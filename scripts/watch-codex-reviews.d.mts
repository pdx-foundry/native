/** A normalized GitHub pull request coordinate. */
export interface PullRequestReference {
  /** Repository owner. */
  readonly owner: string;
  /** Repository name. */
  readonly repository: string;
  /** Pull request number. */
  readonly number: number;
  /** Short coordinate used in poller output. */
  readonly label: string;
  /** Canonical GitHub pull request URL. */
  readonly url: string;
}

/** One inline finding posted by Codex. */
export interface CodexFinding {
  /** GitHub review comment ID. */
  readonly id: number;
  /** Repository-relative source path. */
  readonly path: string;
  /** Current or original diff line when GitHub supplies one. */
  readonly line?: number | null;
  /** Complete Markdown finding text. */
  readonly body: string;
  /** GitHub review comment URL when supplied by the API. */
  readonly url?: string;
}

/** The observed Codex review state for one pull request. */
export interface CodexReviewState {
  /** Pull request being observed. */
  readonly pullRequest: PullRequestReference;
  /** Current lifecycle state reported by the Codex summary comment. */
  readonly status: "waiting" | "running" | "settling" | "completed" | "failed";
  /** Commit abbreviations listed in the current Codex status table. */
  readonly commits: readonly string[];
  /** Last update time of the Codex status comment. */
  readonly summaryUpdatedAt?: string;
  /** Matching GitHub review IDs that supplied findings. */
  readonly reviewIds?: readonly number[];
  /** Terminal status text when Codex reports a failed review. */
  readonly failure?: string;
  /** Inline findings attached to the matching review. */
  readonly findings: readonly CodexFinding[];
}

/** GitHub account identity attached to comments and reviews. */
export interface GitHubUser {
  /** GitHub account login. */
  readonly login?: string;
}

/** Structural subset of a GitHub REST API comment used by the poller. */
export interface GitHubComment {
  /** GitHub comment ID. */
  readonly id: number;
  /** Comment author when returned by GitHub. */
  readonly user?: GitHubUser;
  /** Markdown comment body. */
  readonly body?: string | null;
  /** GitHub creation timestamp. */
  readonly created_at?: string;
  /** GitHub last-update timestamp. */
  readonly updated_at?: string;
  /** Browser URL for the comment. */
  readonly html_url?: string;
  /** Repository-relative path for an inline review comment. */
  readonly path?: string;
  /** Current diff line for an inline review comment. */
  readonly line?: number | null;
  /** Original diff line when the current line is no longer available. */
  readonly original_line?: number | null;
  /** Parent pull request review for an inline review comment. */
  readonly pull_request_review_id?: number | null;
}

/** Structural subset of a GitHub REST API pull request review used by the poller. */
export interface GitHubReview {
  /** GitHub pull request review ID. */
  readonly id: number;
  /** Review author when returned by GitHub. */
  readonly user?: GitHubUser;
  /** Markdown top-level review body. */
  readonly body?: string | null;
  /** Full commit SHA reviewed by GitHub. */
  readonly commit_id?: string;
  /** GitHub review submission timestamp. */
  readonly submitted_at?: string;
}

/** Structural subset of a GitHub REST API reaction used by the poller. */
export interface GitHubReaction {
  /** GitHub reaction ID. */
  readonly id: number;
  /** Reaction author when returned by GitHub. */
  readonly user?: GitHubUser;
  /** GitHub reaction kind, such as `+1` or `eyes`. */
  readonly content?: string;
  /** GitHub reaction creation timestamp. */
  readonly created_at?: string;
}

/** Testable effect boundaries and timing controls for the polling loop. */
export interface WatchCodexReviewOptions {
  /** Delay between polls in milliseconds, from 1 through Node's maximum timer delay. */
  readonly intervalMs?: number;
  /** Return after one request instead of waiting for completion. */
  readonly once?: boolean;
  /** Loads one pull request state. Defaults to the authenticated GitHub CLI. */
  readonly loadState?: (pullRequest: PullRequestReference) => Promise<CodexReviewState>;
  /** Receives each changed report without a trailing newline. */
  readonly write?: (message: string) => void;
  /** Wait implementation used between polling rounds. */
  readonly sleep?: (durationMs: number) => Promise<void>;
}

/** Parses a GitHub pull request URL or an `owner/repository#number` reference. */
export function parsePullRequestReference(reference: string): PullRequestReference;

/**
 * Derives current Codex activity and findings from GitHub REST API objects.
 * Human-authored issue comments must include any `@codex review` triggers so same-head attempts
 * can be distinguished.
 * @param observedAt ISO timestamp used to bound result propagation before reporting no findings.
 */
export function deriveCodexReviewState(
  pullRequest: PullRequestReference,
  issueComments: readonly GitHubComment[],
  reviews: readonly GitHubReview[],
  reviewComments: readonly GitHubComment[],
  headCommit?: string,
  reactions?: readonly GitHubReaction[],
  observedAt?: string
): CodexReviewState;

/** Formats one changed Codex state as concise, agent-readable text. */
export function formatCodexReviewState(state: CodexReviewState): string;

/** Polls pull requests, writes changed states, and stops when all reviews are terminal. */
export function watchCodexReviews(
  pullRequests: readonly PullRequestReference[],
  options?: WatchCodexReviewOptions
): Promise<readonly CodexReviewState[]>;
