use crate::commands::find_upstream;
use crate::stack::{StackBranch, get_all_stack_branches, visualize_stack};
use anyhow::{Context, Result, anyhow};
use git2::{Oid, Repository};
use std::collections::HashMap;
use std::process::Command;

pub fn move_cmd(onto: Option<&str>) -> Result<()> {
    let repo = Repository::open(".").context("Failed to open git repository.")?;

    let upstream_name = find_upstream(&repo)?;
    let upstream_obj = repo.revparse_single(&upstream_name)?;
    let upstream_id = upstream_obj.id();
    let head = repo.head()?;
    let head_id = head.peel_to_commit()?.id();

    let current_branch_name = if !repo.head_detached()? {
        head.shorthand().map(|s| s.to_string())
    } else {
        None
    }
    .ok_or_else(|| anyhow!("You must be on a branch to use 'move'"))?;

    let merge_base = repo.merge_base(upstream_id, head_id)?;
    let all_branches = get_all_stack_branches(&repo, merge_base, &upstream_name)?;

    let selected_target_name = if let Some(target) = onto {
        target.to_string()
    } else {
        let visualized =
            visualize_stack(&repo, merge_base, &all_branches, Some(&current_branch_name))?;

        if visualized.is_empty() {
            println!("No branches found in the stack to move.");
            return Ok(());
        }

        let options: Vec<String> = visualized.iter().map(|v| v.display_name.clone()).collect();
        let selected_display =
            inquire::Select::new("Select target branch to move onto:", options).prompt()?;

        visualized
            .iter()
            .find(|v| v.display_name == selected_display)
            .map(|v| v.name.clone())
            .unwrap()
    };

    if selected_target_name == current_branch_name {
        println!("Already on that branch.");
        return Ok(());
    }

    // 1. Identify sub-stack: current branch and all its descendants in the stack
    let mut sub_stack = Vec::new();
    collect_descendants(&repo, &current_branch_name, &all_branches, &mut sub_stack)?;

    // Sort sub_stack by ancestry so we rebase from bottom to top
    sub_stack.sort_by(|a, b| {
        if a.id == b.id {
            std::cmp::Ordering::Equal
        } else if repo.graph_descendant_of(a.id, b.id).unwrap_or(false) {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Less
        }
    });

    // 2. Map original parents for each branch in the sub-stack
    let mut original_parents = HashMap::new();
    for sb in &sub_stack {
        let parent = find_parent_in_stack(&repo, &sb.name, &all_branches, merge_base)?;
        original_parents.insert(sb.name.clone(), parent);
    }

    println!(
        "Moving stack starting at {} onto {}...",
        current_branch_name, selected_target_name
    );

    // 3. Sequential rebase
    for sb in &sub_stack {
        let old_parent_id = original_parents.get(&sb.name).unwrap();

        let new_base = if sb.name == current_branch_name {
            selected_target_name.clone()
        } else {
            let parent_branch = sub_stack.iter().find(|p| p.id == *old_parent_id);
            match parent_branch {
                Some(p) => p.name.clone(),
                None => old_parent_id.to_string(),
            }
        };

        println!("Rebasing {} onto {}...", sb.name, new_base);
        let status = Command::new("git")
            .arg("rebase")
            .arg("--onto")
            .arg(&new_base)
            .arg(old_parent_id.to_string())
            .arg(&sb.name)
            .status()?;

        if !status.success() {
            return Err(anyhow!(
                "Rebase failed for branch {}. Please resolve manually.",
                sb.name
            ));
        }
    }

    Command::new("git")
        .arg("checkout")
        .arg(&current_branch_name)
        .status()?;

    Ok(())
}

fn collect_descendants(
    repo: &Repository,
    root_name: &str,
    all_branches: &[StackBranch],
    result: &mut Vec<StackBranch>,
) -> Result<()> {
    let root = all_branches.iter().find(|b| b.name == root_name).unwrap();
    result.push(root.clone());

    for b in all_branches {
        if b.name != root_name
            && repo.graph_descendant_of(b.id, root.id).unwrap_or(false)
            && !result.iter().any(|existing| existing.name == b.name)
        {
            result.push(b.clone());
        }
    }
    Ok(())
}

fn find_parent_in_stack(
    repo: &Repository,
    branch_name: &str,
    all_branches: &[StackBranch],
    merge_base: Oid,
) -> Result<Oid> {
    let branch = all_branches.iter().find(|b| b.name == branch_name).unwrap();

    let mut best_parent = merge_base;
    for b in all_branches {
        if b.name != branch_name
            && (repo.graph_descendant_of(branch.id, b.id).unwrap_or(false) || branch.id == b.id)
        {
            if b.id == branch.id {
                continue;
            }
            if best_parent == merge_base
                || repo.graph_descendant_of(b.id, best_parent).unwrap_or(false)
            {
                best_parent = b.id;
            }
        }
    }
    Ok(best_parent)
}
