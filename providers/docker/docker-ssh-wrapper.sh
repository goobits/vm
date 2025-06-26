#!/bin/bash
# Docker SSH wrapper with double Ctrl+C to exit

CTRL_C_COUNT=0
RESET_PID=""

handle_sigint() {
    CTRL_C_COUNT=$((CTRL_C_COUNT + 1))
    
    # Kill the reset timer if it exists
    if [ -n "$RESET_PID" ]; then
        kill $RESET_PID 2>/dev/null
    fi
    
    if [ $CTRL_C_COUNT -eq 1 ]; then
        echo ""
        echo "Press Ctrl+C again to exit the VM session"
        
        # Start reset timer in background
        (sleep 2 && kill -USR1 $$ 2>/dev/null) &
        RESET_PID=$!
    else
        echo ""
        echo "Exiting VM session..."
        exit 0
    fi
}

# Reset counter on USR1 signal
handle_reset() {
    CTRL_C_COUNT=0
    RESET_PID=""
}

# Set up signal handlers
trap handle_sigint INT
trap handle_reset USR1

# Change to workspace and start zsh
cd /workspace
exec /bin/zsh