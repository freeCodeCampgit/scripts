import "dotenv/config";
import fs from "fs";
import { join } from "path";
import render from "mustache";
import GhostAdminAPI from "@tryghost/admin-api";
import yargs from "yargs";
import { hideBin } from "yargs/helpers";
import { fileURLToPath } from "url";
import { dirname } from "path";

import MarkdownRendererFactory from "../lib/index.js";
import logger from "../lib/utils/logger.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const wait = (seconds) =>
  new Promise((resolve) => setTimeout(resolve, seconds * 1000));

const apiUrl = process.env.GHOST_API_URL;
const apiKey = process.env.GHOST_API_KEY;
const apiVersion = "v3.39";

if (!apiUrl || !apiKey) {
  logger.error(
    "GHOST_API_URL and GHOST_API_KEY must be set in the environment variables."
  );
  process.exit(1);
}

const api = new GhostAdminAPI({
  url: apiUrl,
  key: apiKey,
  version: apiVersion,
});

const figureTemplate = `
<figure>
  <img src="{{Src}}">
  {{#Caption}}<figcaption>{{Caption}}</figcaption>{{/Caption}}
</figure>
`;

function renderImage(src, caption) {
  try {
    const data = { Src: src, Caption: caption };
    return render(figureTemplate, data);
  } catch (error) {
    logger.error(`Error rendering image: ${error.message}`);
    return "";
  }
}

function convert(doc, useFigure) {
  try {
    const mobiledoc = JSON.parse(doc.mobiledoc, null, 2);

    // Use the Markdown renderer
    const renderer = new MarkdownRendererFactory().render(mobiledoc);
    let markdown = renderer;

    if (useFigure) {
      markdown = markdown.replace(
        /!\[(.*?)\]\((.*?)\)/g,
        (match, caption, src) => {
          return renderImage(src, caption);
        }
      );
    }

    if (doc.title) {
      markdown = `${doc.header}\n\n# ${doc.title}\n\n` + markdown;
    } else {
      markdown = `${doc.header}\n\n` + markdown;
    }

    return markdown;
  } catch (error) {
    logger.error(
      `Error converting document (slug: ${doc.slug}): ${error.message}`
    );
    return "";
  }
}

function createPostHeaderTemplate(post) {
  const { title, published_at, slug, feature_image, authors, tags } = post;

  const headerTemplate = `---
title: "${title}"
date: "${published_at}"
slug: "${slug}"
feature_image: "${feature_image ?? ""}"
author:
  name: ${authors[0].name}
  slug: ${authors[0].slug}
${
  (tags ?? []).length > 0
    ? `tags:\n${tags
        .map(
          (tag) =>
            "   - name: " + tag.name + "\n" + "     slug: " + tag.slug + "\n"
        )
        .join("")}`
    : ""
}
---
`;

  return headerTemplate.replace(/(^[ \t]*\n)/gm, "");
}

function savePostAsMarkdown(post, useFigure) {
  try {
    const doc = {
      header: createPostHeaderTemplate(post),
      title: post.title,
      mobiledoc: post.mobiledoc,
      slug: post.slug,
    };
    const markdown = convert(doc, useFigure);
    const postStatus = post.status;
    // NOTE: Why not use the primary_author field here?
    const authorName = post.authors[0]?.slug || "unknown-author";
    if (authorName === "unknown-author") {
      logger.error(
        `Post "${post.title}" (slug: ${post.slug}) has no author slug`
      );
    }
    const dirPath = join(__dirname, "..", "__out__", authorName, postStatus);

    if (!fs.existsSync(dirPath)) {
      fs.mkdirSync(dirPath, { recursive: true });
    }

    const filePath = join(dirPath, `${post.slug}.md`);
    fs.writeFileSync(filePath, markdown, "utf8");
    // const mobileDocFilePath = join(dirPath, `${post.slug}.json`);
    // fs.writeFileSync(
    //   mobileDocFilePath,
    //   doc.mobiledoc,
    //   "utf8"
    // );
    logger.info(`Saved post "${post.title}" (slug: ${post.slug})`);
  } catch (error) {
    logger.error(
      `Error saving post "${post.title}" (slug: ${post.slug}): ${error.message}`
    );
  }
}

async function fetchAndSavePostBySlug(slug, useFigure) {
  try {
    const post = await api.posts.read(
      { slug: slug },
      { formats: ["html", "mobiledoc"], include: "authors,tags" }
    );

    logger.info(`Fetched post "${post.title}" (slug: ${slug})`);

    savePostAsMarkdown(post, useFigure);
  } catch (error) {
    logger.error(`Error fetching post with slug "${slug}": ${error.message}`);
  }
}

async function fetchAndSaveAllPosts(
  useFigure,
  batchSize = 10,
  postType,
  authorSlug
) {
  let currPage = 1;
  let lastPage = 1;
  let postsAdded = 0;
  let postsFailed = 0;
  const filters = [];

  if (authorSlug) {
    filters.push(`authors.slug:${authorSlug}`);
  }
  if (postType !== "all") {
    filters.push(`status:${postType}`);
  } else {
    filters.push(`status:[draft,published]`);
  }

  while (currPage && currPage <= lastPage) {
    try {
      const data = await api.posts.browse({
        page: currPage,
        formats: ["html", "mobiledoc"],
        limit: batchSize,
        include: "authors,tags",
        filter: filters.join("+"),
      });
      const posts = [...data];

      if (authorSlug && currPage === 1 && posts.length === 0) {
        logger.error(
          `No posts found for the specified author (author-slug: ${authorSlug}). Is the author-slug correct?`
        );
        return;
      }

      currPage = data.meta.pagination.next;
      lastPage = data.meta.pagination.pages;

      await wait(3);

      for (let post of posts) {
        try {
          logger.info(`Fetched post "${post.title}" (slug: ${post.slug})`);
          savePostAsMarkdown(post, useFigure);
          postsAdded++;
        } catch (err) {
          logger.error(
            `Failed to save post "${post.title}" (slug: ${post.slug}):`,
            err
          );
          postsFailed++;
        }
      }
      logger.info(
        `Completed fetching page ${
          currPage - 1
        }/${lastPage} of posts. ${postsAdded} posts added. ${postsFailed} posts failed.`
      );
    } catch (error) {
      logger.error("Error fetching posts:", error);
      break;
    }
  }

  logger.info(
    `Completed seeding posts. ${postsAdded} posts added. ${postsFailed} posts failed.`
  );
}

const argv = yargs(hideBin(process.argv))
  .option("slug", {
    type: "string",
    description: "The slug of the post to fetch and save",
  })
  .option("author-slug", {
    type: "string",
    description: "The slug of the author whose posts to fetch and save",
  })
  .option("post-type", {
    choices: ["published", "draft", "all"],
    description: "The type of posts to fetch",
    default: "published",
  })
  .option("use-figure", {
    type: "boolean",
    description: "Render images with HTML figure tag",
  })
  .option("batch-size", {
    type: "number",
    description: "Number of posts to fetch in each batch",
    default: 10,
  })
  .help().argv;

if (argv.slug) {
  fetchAndSavePostBySlug(argv.slug, argv.useFigure);
} else if (argv.authorSlug) {
  fetchAndSaveAllPosts(
    argv.useFigure,
    argv.batchSize,
    argv.postType,
    argv.authorSlug
  );
} else {
  fetchAndSaveAllPosts(argv.useFigure, argv.batchSize, argv.postType);
}
