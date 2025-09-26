pub struct OperationMessages {
    pub starting: &'static str,
    pub success: &'static str,
    pub failed: &'static str,
}

pub struct VmOperations {
    pub create: OperationMessages,
    pub start: OperationMessages,
    pub stop: OperationMessages,
    pub destroy: OperationMessages,
}

pub const VM_OPS: VmOperations = VmOperations {
    create: OperationMessages {
        starting: "🚀 Creating '{name}'...",
        success: "✅ Created successfully",
        failed: "❌ Failed to create '{name}'",
    },
    start: OperationMessages {
        starting: "🚀 Starting '{name}'...",
        success: "✅ Started successfully",
        failed: "❌ Failed to start '{name}'",
    },
    stop: OperationMessages {
        starting: "🛑 Stopping '{name}'...",
        success: "✅ Stopped successfully",
        failed: "❌ Failed to stop '{name}'",
    },
    destroy: OperationMessages {
        starting: "🗑️ Destroying '{name}'...",
        success: "✅ Destroyed successfully",
        failed: "❌ Failed to destroy '{name}'",
    },
};
