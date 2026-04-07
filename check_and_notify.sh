#!/bin/bash
# Check for pending test notifications
# Note: This script prepares messages but requires nanobot to be running to actually send them

NOTIFICATION_FILE="$HOME/Development/googleads/mcc-gaql/logs/.notification_pending"
LOGS_DIR="$HOME/Development/googleads/mcc-gaql/logs"

if [ -f "$NOTIFICATION_FILE" ]; then
    # Read notification content
    STATUS=$(grep "Status:" "$NOTIFICATION_FILE" | cut -d: -f2 | xargs)
    TEST_RUN=$(grep "Test Run:" "$NOTIFICATION_FILE" | cut -d: -f2 | xargs)
    LOG_FILE=$(grep "Log File:" "$NOTIFICATION_FILE" | cut -d: -f2 | xargs)
    
    # Create a human-readable summary file
    SUMMARY_FILE="$LOGS_DIR/notification_summary_$TEST_RUN.txt"
    
    if [ "$STATUS" = "PASSED" ]; then
        cat > "$SUMMARY_FILE" << EOF
========================================
mcc-gaql Test Results - ✅ PASSED
========================================

Test Run: $TEST_RUN
Status: All tests passed successfully
Time: $(date)
Log File: $LOG_FILE

Claude completed the test run without errors.
EOF
    else
        # Get last 30 lines of log for error context
        ERROR_CONTEXT=$(tail -30 "$LOG_FILE" 2>/dev/null || echo "Log not available")
        
        cat > "$SUMMARY_FILE" << EOF
========================================
mcc-gaql Test Results - ❌ FAILED
========================================

Test Run: $TEST_RUN
Status: Tests failed or errors found
Time: $(date)
Log File: $LOG_FILE

Last 30 lines of output:
----------------------------------------
$ERROR_CONTEXT
----------------------------------------
EOF
    fi
    
    # Remove notification file
    rm -f "$NOTIFICATION_FILE"
    
    # Output summary to stdout as well
    cat "$SUMMARY_FILE"
fi
