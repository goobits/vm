use anyhow::Result;
use crate::commands::CommandCtx;

pub fn run(ctx: &CommandCtx) -> Result<()> {
    ctx.provider.start()
}

