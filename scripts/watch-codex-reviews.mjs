import { execFile } from "node:child_process";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs, promisify } from "node:util";

const execFileAsync = promisify(execFile);
const CODEX_LOGIN = "chatgpt-codex-connector[bot]";
const CODEX_SUMMARY_MARKER = "<!-- codex-pull-request-review-summary -->";
const CODEX_REVIEW_MARKER = "### 💡 Codex Review";
const DEFAULT_INTERVAL_SECONDS = 30;
const MAX_TIMER_DELAY_MS = 2_147_483_647;
const NO_FINDINGS_SETTLE_MS = 60_000;

function isCodexAuthored(value) {
  return value?.user?.login === CODEX_LOGIN;
}

function compareTimestamps(left, right) {
  return String(left ?? "").localeCompare(String(right ?? ""));
}

function reviewRows(body) {
  return body
    .split(/\r?\n/u)
    .map((line) => line.trim())
    .filter((line) => line.startsWith("|") && line.endsWith("|"))
    .map((line) =>
      line
        .slice(1, -1)
        .split("|")
        .map((cell) => cell.trim())
    )
    .filter((cells) => /\*\*[^*]*Review\*\*/iu.test(cells[0] ?? ""));
}

function commitsFromRows(rows) {
  return [
    ...new Set(
      rows
        .map((cells) => /`([0-9a-f]{7,40})`/iu.exec(cells[2] ?? "")?.[1])
        .filter((commit) => commit !== undefined)
    ),
  ];
}

function latestBy(items, field) {
  return [...items].sort((left, right) => compareTimestamps(right[field], left[field]))[0];
}

function latestReviewTriggerAt(issueComments) {
  const trigger = latestBy(
    issueComments.filter(
      (comment) =>
        !isCodexAuthored(comment) &&
        /(?:^|\s)@codex\s+(?:security\s+)?review\b/iu.test(comment.body ?? "")
    ),
    "created_at"
  );
  return trigger?.created_at;
}

/** Parses a GitHub pull request URL or an `owner/repository#number` reference. */
export function parsePullRequestReference(reference) {
  const shorthand = /^([^/\s]+)\/([^/#\s]+)#([1-9]\d*)$/u.exec(reference);
  if (shorthand !== null) {
    const [, owner, repository, number] = shorthand;
    return {
      owner,
      repository,
      number: Number(number),
      label: `${owner}/${repository}#${number}`,
      url: `https://github.com/${owner}/${repository}/pull/${number}`,
    };
  }

  let url;
  try {
    url = new URL(reference);
  } catch {
    throw new Error(
      `Invalid pull request ${JSON.stringify(reference)}; use a GitHub PR URL or owner/repository#number.`
    );
  }
  const [, owner, repository, kind, number, ...rest] = url.pathname.split("/");
  if (
    !["github.com", "www.github.com"].includes(url.hostname) ||
    owner === undefined ||
    owner === "" ||
    repository === undefined ||
    repository === "" ||
    kind !== "pull" ||
    number === undefined ||
    !/^[1-9]\d*$/u.test(number) ||
    rest.some((segment) => segment !== "")
  ) {
    throw new Error(`Invalid GitHub pull request URL: ${reference}`);
  }
  return {
    owner,
    repository,
    number: Number(number),
    label: `${owner}/${repository}#${number}`,
    url: `https://github.com/${owner}/${repository}/pull/${number}`,
  };
}

/**
 * Derives the current Codex activity and findings from GitHub REST API objects.
 * Human-authored comments are ignored except for `@codex review` attempt boundaries.
 * Findings from older commits and attempts are ignored.
 * @param observedAt ISO timestamp used to bound result propagation before reporting no findings.
 */
export function deriveCodexReviewState(
  pullRequest,
  issueComments,
  reviews,
  reviewComments,
  headCommit,
  reactions = [],
  observedAt
) {
  const summaries = issueComments.filter(
    (comment) => isCodexAuthored(comment) && comment.body?.includes(CODEX_SUMMARY_MARKER)
  );
  const summary = latestBy(summaries, "updated_at");
  if (summary === undefined) {
    return { pullRequest, status: "waiting", commits: [], findings: [] };
  }

  const rows = reviewRows(summary.body ?? "");
  const commits = commitsFromRows(rows);
  const triggerAt = latestReviewTriggerAt(issueComments);
  const staleAttempt =
    typeof triggerAt === "string" && compareTimestamps(summary.updated_at, triggerAt) < 0;
  const staleHead =
    headCommit !== undefined && !commits.some((commit) => headCommit.startsWith(commit));
  if (staleAttempt || staleHead) {
    return {
      pullRequest,
      status: "waiting",
      commits: headCommit === undefined ? commits : [headCommit.slice(0, 7)],
      findings: [],
    };
  }
  const failedRows = rows.filter((cells) =>
    /\*\*(?:Failed|Cancelled|Canceled|Error|Timed out)\*\*/iu.test(cells[1] ?? "")
  );
  const allTerminal =
    rows.length > 0 &&
    rows.every((cells) => /\*\*Completed\*\*/iu.test(cells[1] ?? "") || failedRows.includes(cells));
  if (!allTerminal) {
    return {
      pullRequest,
      status: "running",
      commits,
      summaryUpdatedAt: summary.updated_at,
      findings: [],
    };
  }
  if (failedRows.length > 0) {
    return {
      pullRequest,
      status: "failed",
      commits,
      summaryUpdatedAt: summary.updated_at,
      failure: failedRows.map((cells) => cells[1]).join(", "),
      findings: [],
    };
  }
  const matchingReviews = reviews.filter(
    (review) =>
      isCodexAuthored(review) &&
      review.body?.includes(CODEX_REVIEW_MARKER) &&
      commits.some((commit) => review.commit_id?.startsWith(commit)) &&
      (triggerAt === undefined || compareTimestamps(review.submitted_at, triggerAt) >= 0)
  );
  const reviewIds = new Set(matchingReviews.map((review) => review.id));
  const matchingComments = reviewComments
    .filter(
      (comment) =>
        isCodexAuthored(comment) &&
        comment.pull_request_review_id !== null &&
        comment.pull_request_review_id !== undefined &&
        reviewIds.has(comment.pull_request_review_id)
    )
    .sort((left, right) => compareTimestamps(left.created_at, right.created_at));
  const unpublishedReview = matchingReviews.some(
    (review) => !matchingComments.some((comment) => comment.pull_request_review_id === review.id)
  );
  if (unpublishedReview) {
    return {
      pullRequest,
      status: "settling",
      commits,
      summaryUpdatedAt: summary.updated_at,
      findings: [],
    };
  }
  const noFindings = reactions.some(
    (reaction) =>
      isCodexAuthored(reaction) &&
      reaction.content === "+1" &&
      typeof reaction.created_at === "string" &&
      typeof summary.updated_at === "string" &&
      compareTimestamps(reaction.created_at, summary.updated_at) >= 0
  );
  const completedAt = Date.parse(summary.updated_at ?? "");
  const observedTime = Date.parse(observedAt ?? "");
  const settleWindowElapsed =
    Number.isFinite(completedAt) &&
    Number.isFinite(observedTime) &&
    observedTime - completedAt >= NO_FINDINGS_SETTLE_MS;
  if (matchingReviews.length === 0 && !noFindings && !settleWindowElapsed) {
    return {
      pullRequest,
      status: "settling",
      commits,
      summaryUpdatedAt: summary.updated_at,
      findings: [],
    };
  }
  const findings = matchingComments.map((comment) => {
    if (typeof comment.path !== "string") {
      throw new Error(`Codex review comment ${comment.id} has no source path.`);
    }
    return {
      id: comment.id,
      path: comment.path,
      line: comment.line ?? comment.original_line,
      body: comment.body ?? "",
      url: comment.html_url,
    };
  });

  return {
    pullRequest,
    status: "completed",
    commits,
    summaryUpdatedAt: summary.updated_at,
    reviewIds: matchingReviews.map((review) => review.id),
    findings,
  };
}

/** Formats one changed Codex state as concise, agent-readable text. */
export function formatCodexReviewState(state) {
  const prefix = `[${state.pullRequest.label}]`;
  const commit = state.commits.length === 0 ? "" : ` for ${state.commits.join(", ")}`;
  if (state.status === "waiting") {
    return `${prefix} Waiting for Codex review${commit} to start.`;
  }
  if (state.status === "running") {
    return `${prefix} Codex review is running${commit}.`;
  }
  if (state.status === "settling") {
    return `${prefix} Codex review completed${commit}; waiting for GitHub to publish the result.`;
  }
  if (state.status === "failed") {
    return `${prefix} Codex review failed${commit}: ${state.failure ?? "unknown failure"}.`;
  }
  if (state.findings.length === 0) {
    return `${prefix} Codex review completed${commit} with no findings.`;
  }

  const count = state.findings.length;
  const heading = `${prefix} Codex review completed${commit} with ${count} finding${count === 1 ? "" : "s"}.`;
  const findings = state.findings.map((finding) => {
    const location = finding.line == null ? finding.path : `${finding.path}:${finding.line}`;
    return `${location}\n${finding.body}`;
  });
  return [heading, ...findings].join("\n\n");
}

async function githubApiPages(endpoint) {
  const { stdout } = await execFileAsync("gh", ["api", "--paginate", "--slurp", endpoint], {
    encoding: "utf8",
    maxBuffer: 20 * 1024 * 1024,
  });
  const pages = JSON.parse(stdout);
  if (!Array.isArray(pages) || pages.some((page) => !Array.isArray(page))) {
    throw new Error(`GitHub returned an unexpected response for ${endpoint}.`);
  }
  return pages.flat();
}

async function githubApiObject(endpoint) {
  const { stdout } = await execFileAsync("gh", ["api", endpoint], {
    encoding: "utf8",
    maxBuffer: 20 * 1024 * 1024,
  });
  const value = JSON.parse(stdout);
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`GitHub returned an unexpected response for ${endpoint}.`);
  }
  return value;
}

async function loadCodexReviewState(pullRequest) {
  const base = `repos/${encodeURIComponent(pullRequest.owner)}/${encodeURIComponent(pullRequest.repository)}`;
  const [issueComments, pull] = await Promise.all([
    githubApiPages(`${base}/issues/${pullRequest.number}/comments?per_page=100`),
    githubApiObject(`${base}/pulls/${pullRequest.number}`),
  ]);
  if (pull.draft === true) {
    throw new Error(`${pullRequest.label} is a draft pull request; Codex does not review drafts.`);
  }
  const headCommit = pull.head?.sha;
  if (typeof headCommit !== "string") {
    throw new Error(`GitHub returned no head commit for ${pullRequest.label}.`);
  }
  const observedAt = new Date().toISOString();
  const status = deriveCodexReviewState(
    pullRequest,
    issueComments,
    [],
    [],
    headCommit,
    [],
    observedAt
  );
  if (status.status !== "settling" && status.status !== "completed") {
    return status;
  }

  const [reviews, reviewComments, reactions] = await Promise.all([
    githubApiPages(`${base}/pulls/${pullRequest.number}/reviews?per_page=100`),
    githubApiPages(`${base}/pulls/${pullRequest.number}/comments?per_page=100`),
    githubApiPages(`${base}/issues/${pullRequest.number}/reactions?per_page=100`),
  ]);
  return deriveCodexReviewState(
    pullRequest,
    issueComments,
    reviews,
    reviewComments,
    headCommit,
    reactions,
    observedAt
  );
}

function isTerminal(state) {
  return state?.status === "completed" || state?.status === "failed";
}

/**
 * Polls several pull requests, writes only changed states, and stops when all reviews are terminal.
 * Set `once` to inspect each pull request once without waiting.
 */
export async function watchCodexReviews(pullRequests, options = {}) {
  const intervalMs = options.intervalMs ?? DEFAULT_INTERVAL_SECONDS * 1_000;
  if (!Number.isFinite(intervalMs) || intervalMs < 1 || intervalMs > MAX_TIMER_DELAY_MS) {
    throw new Error(
      `Polling interval must be between 1 and ${MAX_TIMER_DELAY_MS} milliseconds; received ${String(intervalMs)}.`
    );
  }
  const loadState = options.loadState ?? loadCodexReviewState;
  const write = options.write ?? ((message) => process.stdout.write(`${message}\n`));
  const sleep =
    options.sleep ??
    ((duration) => new Promise((resolveSleep) => setTimeout(resolveSleep, duration)));
  const previous = new Map();
  const statesByPullRequest = new Map();

  while (true) {
    const activePullRequests = pullRequests.filter(
      (pullRequest) => !isTerminal(statesByPullRequest.get(pullRequest.label))
    );
    const loadedStates = await Promise.all(
      activePullRequests.map((pullRequest) => loadState(pullRequest))
    );
    for (const state of loadedStates) {
      statesByPullRequest.set(state.pullRequest.label, state);
    }
    const states = pullRequests.map((pullRequest) => statesByPullRequest.get(pullRequest.label));
    const changedReports = states
      .map((state) => ({ state, report: formatCodexReviewState(state) }))
      .filter(({ state, report }) => {
        const oldReport = previous.get(state.pullRequest.label);
        previous.set(state.pullRequest.label, report);
        return report !== oldReport;
      })
      .map(({ report }) => report);
    if (changedReports.length > 0) {
      write(changedReports.join("\n\n"));
    }
    const allTerminal = states.every(isTerminal);
    if (options.once === true || allTerminal) {
      return states;
    }
    await sleep(intervalMs);
  }
}

function usage() {
  return "Usage: node scripts/watch-codex-reviews.mjs [--interval <seconds>] [--once] <PR URL | owner/repository#number> [...]";
}

async function main(args) {
  const { values, positionals } = parseArgs({
    args,
    allowPositionals: true,
    options: {
      help: { type: "boolean", short: "h" },
      interval: { type: "string", default: String(DEFAULT_INTERVAL_SECONDS) },
      once: { type: "boolean", default: false },
    },
    strict: true,
  });
  if (values.help === true) {
    console.log(usage());
    return;
  }
  if (positionals.length === 0) {
    throw new Error(usage());
  }
  const intervalSeconds = Number(values.interval);
  if (!Number.isFinite(intervalSeconds) || intervalSeconds <= 0) {
    throw new Error(
      `--interval must be a positive number; received ${JSON.stringify(values.interval)}.`
    );
  }
  const pullRequests = [
    ...new Map(
      positionals.map((reference) => {
        const pullRequest = parsePullRequestReference(reference);
        return [pullRequest.label, pullRequest];
      })
    ).values(),
  ];
  await watchCodexReviews(pullRequests, {
    intervalMs: intervalSeconds * 1_000,
    once: values.once,
  });
}

if (process.argv[1] !== undefined && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  try {
    await main(process.argv.slice(2));
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
