use anyhow::Result;
use std::path::Path;
use crate::commands::CommandCtx;

pub fn run(ctx: &CommandCtx, relative_path: &Path) -> Result<()> {
    ctx.provider.ssh(relative_path)
}

