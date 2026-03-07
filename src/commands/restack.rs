use crate::rebase_utils::{Operation, RebaseState, run_rebase_loop, state_path};
use anyhow::{Result, anyhow};
use git2::{BranchType, Commit, Oid, Repository};
use std::collections::HashMap;

pub fn restack() -> Result<()> {
    let repo = crate::open_repo()?;

    if state_path(&repo).exists() {
        return Err(anyhow!("A rebase operation is already in progress."));
    }

    let head = repo.head()?;
    let current_branch_name = head
        .shorthand()
        .ok_or_else(|| anyhow!("Detached HEAD"))?
        .to_string();
    let head_commit = head.peel_to_commit()?;

    println!(
        "Finding branches to restack onto '{}'...",
        current_branch_name
    );

    let children = find_floating_children(&repo, &head_commit, &current_branch_name)?;

    if children.is_empty() {
        println!("No floating children found.");
        return Ok(());
    }

    // Construct RebaseState
    let mut parent_id_map = HashMap::new();
    let mut parent_name_map = HashMap::new();
    let mut remaining = Vec::new();

    for (name, old_base) in children {
        println!(" - {} (matches old base {})", name, old_base);
        remaining.push(name.clone());
        parent_id_map.insert(name.clone(), old_base.to_string());
        parent_name_map.insert(name.clone(), current_branch_name.clone());
    }

    let state = RebaseState {
        operation: Operation::Move,
        original_branch: current_branch_name.clone(),
        target_branch: current_branch_name.clone(),
        caller_branch: Some(current_branch_name.clone()),
        remaining_branches: remaining,
        in_progress_branch: None,
        parent_id_map,
        parent_name_map,
        stash_ref: None,
        unstage_on_restore: false,
    };

    crate::rebase_utils::save_state(&repo, &state)?;
    run_rebase_loop(&repo, state)?;

    Ok(())
}

fn find_floating_children(
    repo: &Repository,
    head_commit: &Commit,
    current_branch: &str,
) -> Result<Vec<(String, Oid)>> {
    let mut results = Vec::new();
    let head_id = head_commit.id();
    let head_email = head_commit.author().email().unwrap_or("").to_string();
    let head_summary = head_commit.summary().unwrap_or("").trim().to_string();

    let branches = repo.branches(Some(BranchType::Local))?;

    for branch_res in branches {
        let (branch, _) = branch_res?;
        let name = match branch.name() {
            Ok(Some(n)) => n.to_string(),
            _ => continue,
        };

        if name == current_branch {
            continue;
        }

        let tip = match branch.get().target() {
            Some(t) => t,
            None => continue,
        };

        // If the branch is already a descendant of HEAD, it doesn't need restacking
        if repo.graph_descendant_of(tip, head_id)? || tip == head_id {
            continue;
        }

        // Walk back to find a commit that matches HEAD's metadata
        let mut walk = repo.revwalk()?;
        walk.push(tip)?;
        walk.set_sorting(git2::Sort::TOPOLOGICAL)?;

        // We limit the search depth to avoid scanning the entire history of huge repos.
        // 100 commits should be sufficient for finding the divergence point of a recent rebase.
        for oid_res in walk.take(100) {
            let oid = oid_res?;

            // Optimization: If we hit a commit that is reachable from HEAD, we stop.
            // Because any match found *after* this point would be a common ancestor, not a floating base.
            if repo.graph_descendant_of(head_id, oid)? {
                break;
            }

            if oid == head_id {
                break;
            }

            let commit = repo.find_commit(oid)?;

            let email = commit.author().email().unwrap_or("").to_string();
            let summary = commit.summary().unwrap_or("").trim().to_string();

            // Match Criteria
            if email == head_email && summary == head_summary {
                results.push((name.clone(), oid));
                break;
            }
        }
    }
    Ok(results)
}
