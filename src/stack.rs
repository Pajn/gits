use anyhow::{Result, anyhow};
use git2::{BranchType, Oid, Repository};

pub struct StackBranch {
    pub name: String,
    pub id: Oid,
}

pub fn get_stack_branches(
    repo: &Repository,
    head_id: Oid,
    upstream_id: Oid,
    upstream_name: &str,
) -> Result<Vec<StackBranch>> {
    let mut stack_branches = Vec::new();
    let local_branches = repo.branches(Some(BranchType::Local))?;

    for res in local_branches {
        let (branch, _) = res?;
        let name = branch
            .name()?
            .ok_or_else(|| anyhow!("Branch has no name"))?
            .to_string();

        if name == upstream_name {
            continue;
        }

        if let Some(target_id) = branch.get().target() {
            let is_linear = repo.graph_descendant_of(head_id, target_id)?
                || repo.graph_descendant_of(target_id, head_id)?
                || head_id == target_id;

            let is_merged = repo.graph_descendant_of(upstream_id, target_id)?;

            if is_linear && !is_merged {
                stack_branches.push(StackBranch {
                    name,
                    id: target_id,
                });
            }
        }
    }

    Ok(stack_branches)
}
