use crate::rebase_utils::{Operation, load_state};
use anyhow::{Context, Result};
use git2::Repository;

pub fn status_cmd() -> Result<()> {
    let repo = Repository::open(".").context("Failed to open git repository.")?;
    let state = load_state(&repo)?;
    let op_name = match state.operation {
        Operation::Move => "Move",
        Operation::Commit => "Commit",
    };
    println!(
        "{} in progress: {} onto {}",
        op_name, state.original_branch, state.target_branch
    );
    println!(
        "Remaining branches: {}",
        state.remaining_branches.join(", ")
    );
    Ok(())
}
