use crate::commands::find_upstream;
use crate::stack::get_stack_branches;
use anyhow::{Context, Result, anyhow};
use git2::Repository;
use std::process::Command;

pub fn checkout() -> Result<()> {
    let repo = Repository::open(".").context("Failed to open git repository.")?;

    let upstream_name = find_upstream(&repo)?;
    let upstream_obj = repo.revparse_single(&upstream_name)?;
    let upstream_id = upstream_obj.id();
    let head_id = repo.head()?.peel_to_commit()?.id();

    let branches = get_stack_branches(&repo, head_id, upstream_id, &upstream_name)?;
    let mut branch_names: Vec<String> = branches.into_iter().map(|b| b.name).collect();

    if branch_names.is_empty() {
        println!(
            "No branches found in the current stack (excluding {}).",
            upstream_name
        );
        return Ok(());
    }

    branch_names.sort();

    let selected = inquire::Select::new("Select branch to checkout:", branch_names).prompt()?;

    let status = Command::new("git")
        .arg("checkout")
        .arg(&selected)
        .status()?;

    if !status.success() {
        return Err(anyhow!("git checkout failed"));
    }

    Ok(())
}
