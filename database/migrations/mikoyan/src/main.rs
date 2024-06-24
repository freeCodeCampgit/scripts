use clap::Parser;
use db::get_collection;
use futures_util::TryStreamExt;
use mongodb::{bson::doc, options::FindOptions};
use tokio::{self, io::AsyncWriteExt, task::JoinHandle};

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

mod clapper;
mod convert;
mod db;
mod error;
mod normalize;
mod record;

use error::Error;
use normalize::{normalize_user, NormalizeError};

use clapper::Args;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args = Args::parse();

    let num_threads = if let Some(num_threads) = args.num_threads {
        num_threads
    } else {
        1
    };

    let mut handles = Vec::new();

    let num_docs_in_collection = {
        let collection = get_collection(&args.uri, "user").await?;
        collection.estimated_document_count(None).await? as usize
    };

    println!("Docs in user: {}", num_docs_in_collection);

    // Split the database into `num_threads` chunks
    // Any remainder will be handled by the last thread
    let num_docs_per_thread = if let Some(num_docs) = args.num_docs {
        num_docs / num_threads
    } else {
        num_docs_in_collection / num_threads
    };

    let m = MultiProgress::new();
    for thread_id in 0..num_threads {
        let num_docs_to_handle = if thread_id == num_threads - 1 {
            // Handle any remainder
            num_docs_per_thread + num_docs_in_collection % num_threads
        } else {
            num_docs_per_thread
        };

        println!("Thread {}: {:?}", thread_id, num_docs_to_handle);

        let args = args.clone();

        let m_clone = m.clone();
        let handle: JoinHandle<Result<(), mongodb::error::Error>> = tokio::spawn(async move {
            match connect_and_process(args, num_docs_to_handle, thread_id, m_clone).await {
                Ok(_) => Ok(()),
                Err(e) => Err(e),
            }
        });

        handles.push(handle);
    }

    let mut file = tokio::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(args.logs)
        .await?;

    for handle in handles {
        if let Err(e) = handle.await {
            // Write errors to logs file
            file.write_all(format!("{}\n", e).as_bytes()).await?;
        }
    }
    Ok(())
}

async fn connect_and_process(
    args: Args,
    num_docs_to_handle: usize,
    thread_id: usize,
    m: MultiProgress,
) -> Result<(), mongodb::error::Error> {
    let user_collection = get_collection(&args.uri, "user").await?;

    let find_ops = FindOptions::builder()
        .limit(num_docs_to_handle as i64)
        .skip((thread_id * num_docs_to_handle) as u64)
        .batch_size(10)
        .build();
    let mut cursor = user_collection.find(doc! {}, find_ops).await?;

    let sty = ProgressStyle::with_template(
        "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}",
    )
    .unwrap()
    .progress_chars("##-");

    let pb = m.add(ProgressBar::new(num_docs_to_handle as u64));
    pb.set_style(sty);

    let mut logs_file = tokio::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(args.logs)
        .await?;

    let mut count: usize = 0;
    let epoch_size = 1000;
    let epoch = (num_docs_to_handle / epoch_size).max(1);
    while let Some(user) = cursor.try_next().await? {
        match normalize_user(user) {
            Ok(normalized_user) => {
                // _id exists, because `normalize_user` returns an error if it does not
                let id = normalized_user.get_object_id("_id").unwrap();
                let filter = doc! {"_id": id};
                let _res = user_collection
                    .replace_one(filter, normalized_user, None)
                    .await?;
            }
            Err(normalize_error) => {
                // Write to logs file
                // Format: <user_id>: <error>
                match normalize_error {
                    NormalizeError::UnhandledType { id, error } => {
                        logs_file
                            .write_all(format!("{}: {}\n", id, error).as_bytes())
                            .await?;
                    }
                    NormalizeError::ConfusedId { doc } => {
                        logs_file
                            .write_all(format!("{}: {}\n", "Confused ID", doc).as_bytes())
                            .await?;
                    }
                    NormalizeError::NullEmail { doc } => {
                        let id = doc.get_object_id("_id").unwrap();
                        // Add user record to own collection
                        let recovered_users_collection =
                            get_collection(&args.uri, "recovered_users").await?;
                        recovered_users_collection.insert_one(doc, None).await?;

                        // Remove user from normalized database
                        let filter = doc! {"_id": id};
                        user_collection.delete_one(filter, None).await?;
                    }
                }
            }
        }

        count += 1;
        if count % epoch == 0 {
            let per = (count as f64 / epoch as f64) / (epoch_size as f64 * 100.0);
            pb.set_message(format!("{}%", per));
            pb.inc(epoch as u64);
        }
    }

    pb.finish_with_message("done");
    Ok(())
}
