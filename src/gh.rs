use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::process::Command;

/// Verify that the `gh` CLI is installed and authenticated.
pub fn check_gh() -> Result<()> {
    let status = Command::new("gh")
        .arg("auth")
        .arg("status")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => Err(anyhow!(
            "`gh` is not authenticated. Run `gh auth login` first."
        )),
        Err(_) => Err(anyhow!(
            "`gh` CLI not found. Install it from https://cli.github.com/ and run `gh auth login`."
        )),
    }
}

#[derive(Debug, Clone)]
pub struct ExistingPr {
    pub number: u64,
    pub base_branch: String,
}

/// Check if an open PR exists for `branch`. Returns `Some(ExistingPr)` or `None`.
pub fn find_open_pr(branch: &str) -> Result<Option<ExistingPr>> {
    #[derive(Deserialize)]
    struct PrView {
        number: u64,
        #[serde(rename = "baseRefName")]
        base_ref_name: String,
        state: String,
    }

    let output = Command::new("gh")
        .args(["pr", "view", branch, "--json", "number,baseRefName,state"])
        .output()
        .context("Failed to run `gh pr view`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("no pull requests found for branch") {
            return Ok(None);
        }
        return Err(anyhow!("`gh pr view` failed: {}", stderr.trim()));
    }

    let pr: PrView =
        serde_json::from_slice(&output.stdout).context("Failed to parse `gh pr view` output")?;

    if pr.state.to_uppercase() == "OPEN" {
        Ok(Some(ExistingPr {
            number: pr.number,
            base_branch: pr.base_ref_name,
        }))
    } else {
        Ok(None)
    }
}

/// Update the base branch of an existing PR.
pub fn update_pr_base(pr_number: u64, new_base: &str) -> Result<()> {
    let status = Command::new("gh")
        .args(["pr", "edit", &pr_number.to_string(), "--base", new_base])
        .status()
        .context("Failed to run `gh pr edit`")?;

    if !status.success() {
        return Err(anyhow!("Failed to update base for PR #{}", pr_number));
    }
    Ok(())
}

/// Create a new PR. Returns the URL of the created PR.
pub fn create_pr(params: &CreatePrParams) -> Result<String> {
    let mut cmd = Command::new("gh");
    cmd.args([
        "pr",
        "create",
        "--title",
        &params.title,
        "--body",
        &params.body,
        "--base",
        &params.base,
        "--head",
        &params.head,
    ]);

    if params.draft {
        cmd.arg("--draft");
    }

    for label in &params.labels {
        cmd.args(["--label", label]);
    }

    for reviewer in &params.reviewers {
        cmd.args(["--reviewer", reviewer]);
    }

    let output = cmd.output().context("Failed to run `gh pr create`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to create PR: {}", stderr.trim()));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub struct CreatePrParams {
    pub title: String,
    pub body: String,
    pub base: String,
    pub head: String,
    pub draft: bool,
    pub labels: Vec<String>,
    pub reviewers: Vec<String>,
}

/// Fetch all labels available in the repo. Returns a list of label names.
pub fn list_labels() -> Result<Vec<String>> {
    #[derive(Deserialize)]
    struct Label {
        name: String,
    }

    let output = Command::new("gh")
        .args(["label", "list", "--json", "name", "--limit", "100"])
        .output()
        .context("Failed to run `gh label list`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to list labels: {}", stderr.trim()));
    }

    let labels: Vec<Label> =
        serde_json::from_slice(&output.stdout).context("Failed to parse `gh label list` output")?;

    Ok(labels.into_iter().map(|l| l.name).collect())
}

/// Fetch collaborators/assignable users for the current repo.
pub fn list_collaborators() -> Result<Vec<String>> {
    let output = Command::new("gh")
        .args([
            "api",
            "repos/{owner}/{repo}/collaborators",
            "--jq",
            ".[].login",
            "--paginate",
        ])
        .output()
        .context("Failed to run `gh api` for collaborators")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to list collaborators: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let logins: Vec<String> = stdout
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    Ok(logins)
}
