use mongodb::bson::{doc, oid::ObjectId, Bson, Document};

#[derive(Debug)]
pub enum NormalizeError {
    UnhandledType { id: ObjectId, doc: Document },
}

pub fn normalize_user(user: &Document) -> Result<Document, NormalizeError> {
    let empty_vec: mongodb::bson::Array = Vec::new();
    let mut normalize_error = None;

    let mut update_op = doc! {};

    let user_id = if user.get_object_id("_id").is_ok() {
        user.get_object_id("_id").unwrap()
    } else {
        normalize_error = Some(NormalizeError::UnhandledType {
            id: ObjectId::new(),
            doc: user.clone(),
        });
        return Err(normalize_error.unwrap());
    };

    if user.get("savedChallenges").is_none() {
        update_op.insert("savedChallenges", empty_vec.clone());
    }

    if user.get("badges").is_none() {
        update_op.insert("badges", empty_vec.clone());
    }

    if let Some(_partially_completed_challenges) = user.get("partiallyCompletedChallenges") {
        // Handle partial challenge format
    } else {
        update_op.insert("partiallyCompletedChallenges", empty_vec.clone());
    }

    if let Some(_completed_challenges) = user.get("completedChallenges") {
        // Handle completed challenge format
    } else {
        update_op.insert("completedChallenges", empty_vec.clone());
    }

    if let Some(_progress_timestamps) = user.get("progressTimestamps") {
        // Handle progress timestamps format
    } else {
        update_op.insert("progressTimestamps", empty_vec.clone());
    }

    if let Some(years_top_contributor) = user.get("yearsTopContributor") {
        // Handle years top contributor format
        match years_top_contributor {
            Bson::Array(arr) => {
                // Convert `[Bson::String]` to `[Bson::Double]`
                let mut new_arr = Vec::new();
                for year in arr {
                    match year {
                        Bson::String(year) => {
                            if let Ok(year) = year.parse::<f64>() {
                                new_arr.push(Bson::Double(year));
                            } else {
                                normalize_error = Some(NormalizeError::UnhandledType {
                                    id: user_id,
                                    doc: doc! {
                                        "yearsTopContributor": years_top_contributor.clone()
                                    },
                                });
                                break;
                            }
                        }
                        Bson::Double(year) => {
                            new_arr.push(Bson::Double(*year));
                        }
                        Bson::Int32(year) => {
                            new_arr.push(Bson::Double(*year as f64));
                        }
                        Bson::Int64(year) => {
                            new_arr.push(Bson::Double(*year as f64));
                        }
                        _ => {
                            normalize_error = Some(NormalizeError::UnhandledType {
                                id: user_id,
                                doc: doc! {
                                    "yearsTopContributor": years_top_contributor.clone()
                                },
                            });
                            break;
                        }
                    };
                }
                update_op.insert("yearsTopContributor", new_arr);
            }
            Bson::Null => {
                update_op.insert("yearsTopContributor", empty_vec.clone());
            }
            _ => {
                normalize_error = Some(NormalizeError::UnhandledType {
                    id: user_id,
                    doc: doc! {
                        "yearsTopContributor": years_top_contributor.clone()
                    },
                });
            }
        };
    } else {
        update_op.insert("yearsTopContributor", empty_vec.clone());
    }

    if let Some(_profile_ui) = user.get("profileUI") {
        // Handle profile UI format
    } else {
        update_op.insert("profileUI", empty_vec);
    }

    if let Some(normalize_error) = normalize_error {
        Err(normalize_error)
    } else {
        let update_op = doc! {
            "$set": update_op,
            "$unset": doc! {
                "password": "",
                "isGithub": "",
                "isLinkedIn": "",
                "isTwitter": "",
                "isWebsite": "",
                // "github": "",
                // "timezone": "",
                "completedChallenges.$.__cachedRelations": "",
                "completedChallenges.$.__data": "",
                "completedChallenges.$.__dataSource": "",
                "completedChallenges.$.__persisted": "",
                "completedChallenges.$.__strict": "",
                "completedChallenges.$.files.$.__cachedRelations": "",
                "completedChallenges.$.files.$.__data": "",
                "completedChallenges.$.files.$.__dataSource": "",
                "completedChallenges.$.files.$.__persisted": "",
                "completedChallenges.$.files.$.__strict": "",
                "profileUI.$.__cachedRelations": "",
                "profileUI.$.__data": "",
                "profileUI.$.__dataSource": "",
                "profileUI.$.__persisted": "",
                "profileUI.$.__strict": "",
            },
        };
        Ok(update_op)
    }
}