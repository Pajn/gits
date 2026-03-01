use crate::commands::find_upstream;
use crate::stack::get_stack_branches;
use anyhow::{Context, Result, anyhow};
use git2::{BranchType, Repository};
use inquire::MultiSelect;
use std::collections::HashMap;
use std::fmt;
use std::process::Command;

pub fn push() -> Result<()> {
    let repo = Repository::open(".").context("Failed to open git repository.")?;

    // Check remotes first to avoid hanging on interactive prompt if it will fail anyway
    let remote_name = resolve_remote(&repo)?;

    let upstream_name = find_upstream(&repo)?;
    let upstream_obj = repo.revparse_single(&upstream_name)?;
    let upstream_id = upstream_obj.id();
    let head_id = repo.head()?.peel_to_commit()?.id();

    let mut branches_to_push = Vec::new();
    let mut branches_without_upstream = Vec::new();

    let stack_branches = get_stack_branches(&repo, head_id, upstream_id, &upstream_name)?;
    for sb in stack_branches {
        let branch = repo.find_branch(&sb.name, BranchType::Local)?;
        match branch.upstream() {
            Ok(upstream) => {
                let upstream_name = upstream.name()?.unwrap_or("unknown").to_string();
                branches_to_push.push(BranchStatus {
                    name: sb.name,
                    upstream: Some(upstream_name),
                });
            }
            Err(_) => {
                branches_without_upstream.push(BranchStatus {
                    name: sb.name,
                    upstream: None,
                });
            }
        }
    }

    if branches_to_push.is_empty() && branches_without_upstream.is_empty() {
        println!("No branches in stack to push.");
        return Ok(());
    }

    if branches_without_upstream.is_empty() {
        perform_push(&repo, branches_to_push)?;
    } else {
        // Show list and allow marking for upstream
        let mut all_branches = branches_to_push.clone();
        all_branches.extend(branches_without_upstream.clone());
        all_branches.sort_by(|a, b| a.name.cmp(&b.name));

        let options = all_branches
            .iter()
            .filter(|b| b.upstream.is_none())
            .cloned()
            .collect::<Vec<_>>();

        if options.is_empty() {
            perform_push(&repo, branches_to_push)?;
            return Ok(());
        }

        let selected = MultiSelect::new(
            "Select branches to set upstream and push (Space to toggle, Enter to confirm):",
            options,
        )
        .prompt()?;

        if selected.is_empty() && branches_to_push.is_empty() {
            println!("No branches selected to push.");
            return Ok(());
        }

        // Set upstreams for selected branches
        for mut branch_status in selected {
            let mut branch = repo.find_branch(&branch_status.name, BranchType::Local)?;
            let upstream_ref = format!("refs/remotes/{}/{}", remote_name, branch_status.name);
            branch
                .set_upstream(Some(&upstream_ref))
                .context(format!("Failed to set upstream for {}", branch_status.name))?;

            branch_status.upstream = Some(upstream_ref);
            branches_to_push.push(branch_status);
        }

        perform_push(&repo, branches_to_push)?;
    }

    Ok(())
}

#[derive(Clone, Debug)]
struct BranchStatus {
    name: String,
    upstream: Option<String>,
}

impl fmt::Display for BranchStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.upstream {
            Some(u) => write!(f, "{} -> {}", self.name, u),
            None => write!(f, "{} (no upstream)", self.name),
        }
    }
}

fn resolve_remote(repo: &Repository) -> Result<String> {
    let remotes = repo.remotes()?;
    let remote_list: Vec<String> = remotes.iter().flatten().map(|s| s.to_string()).collect();

    if remote_list.contains(&"origin".to_string()) {
        Ok("origin".to_string())
    } else if remote_list.len() == 1 {
        Ok(remote_list[0].clone())
    } else if remote_list.is_empty() {
        Err(anyhow!("No remotes configured."))
    } else {
        Err(anyhow!(
            "'origin' remote not found and multiple remotes exist. Please specify a remote or rename one to 'origin'."
        ))
    }
}

fn perform_push(_repo: &Repository, branches: Vec<BranchStatus>) -> Result<()> {
    if branches.is_empty() {
        println!("Nothing to push.");
        return Ok(());
    }

    // Identify which remotes we are pushing to.
    // Atomic push only works for a single remote at a time.
    let mut remote_to_refs: HashMap<String, Vec<String>> = HashMap::new();
    for b in branches {
        if let Some(upstream) = b.upstream {
            let parts: Vec<&str> = upstream.splitn(2, '/').collect();
            if parts.len() == 2 {
                remote_to_refs
                    .entry(parts[0].to_string())
                    .or_default()
                    .push(b.name);
            }
        }
    }

    for (remote, refs) in remote_to_refs {
        println!("Pushing {} branches to {}...", refs.len(), remote);
        let mut cmd = Command::new("git");
        cmd.arg("push")
            .arg("--atomic")
            .arg("--force-with-lease")
            .arg(&remote);

        for r in refs {
            cmd.arg(format!("{}:{}", r, r));
        }

        let status = cmd.status()?;
        if !status.success() {
            return Err(anyhow!("Push failed for remote '{}'", remote));
        }
    }

    Ok(())
}
