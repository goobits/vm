pub struct Messages {
    // ... existing fields
    pub vm_service_cleanup_success: &'static str,
    // ... rest of the fields
}

pub const MESSAGES: Messages = Messages {
    // ... existing values
    vm_service_cleanup_success: "✅ Services cleaned up successfully",
    // ... rest of the values
};
