use anyhow::Result;
use crate::commands::CommandCtx;

pub fn run(ctx: &CommandCtx, command: &[String]) -> Result<()> {
    ctx.provider.exec(command)
}

