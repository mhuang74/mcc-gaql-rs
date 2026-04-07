#!/bin/bash
# Automated test runner for mcc-gaql using Claude CLI
# Triggered by crontab at: 2:30 AM, 7:35 AM, 12:40 PM, 5:45 PM, 10:50 PM (Asia/Taipei)
# Claude will run tests and investigate root cause of any errors

# Don't use set -e because we need to capture exit codes and create notifications
# set -e

PROJECT_DIR="$HOME/Development/googleads/mcc-gaql"
LOG_DIR="$PROJECT_DIR/logs"
TIMESTAMP=$(date '+%Y%m%d_%H%M%S')
LOG_FILE="$LOG_DIR/claude_test_run_$TIMESTAMP.log"

# Create logs directory if it doesn't exist
mkdir -p "$LOG_DIR"

echo "========================================" >> "$LOG_FILE"
echo "Claude Test Run Started: $(date)" >> "$LOG_FILE"
echo "Working Directory: $PROJECT_DIR" >> "$LOG_FILE"
echo "========================================" >> "$LOG_FILE"
echo "" >> "$LOG_FILE"

cd "$PROJECT_DIR"

# Run Claude CLI to execute tests and investigate errors
echo "Invoking Claude CLI to run tests and investigate errors..." >> "$LOG_FILE"
claude -p "Run all tests in this Rust workspace and investigate root cause of any errors found. Use cargo test --workspace to run tests." --dangerously-skip-permissions --model haiku 3>&1 | tee -a "$LOG_FILE"

CLAUDE_EXIT_CODE=${PIPESTATUS[0]}

# Create notification file for nanobot
NOTIFICATION_FILE="$LOG_DIR/.notification_pending"
TEST_STATUS="FAILED"
if [ $CLAUDE_EXIT_CODE -eq 0 ]; then
    TEST_STATUS="PASSED"
fi

# Write notification payload
cat > "$NOTIFICATION_FILE" << EOF
Test Run: $TIMESTAMP
Status: $TEST_STATUS
Exit Code: $CLAUDE_EXIT_CODE
Log File: $LOG_FILE
Time: $(date)
Project: mcc-gaql
EOF

echo "" >> "$LOG_FILE"
echo "========================================" >> "$LOG_FILE"
if [ $CLAUDE_EXIT_CODE -eq 0 ]; then
    echo "✅ Claude completed successfully: $(date)" >> "$LOG_FILE"
else
    echo "⚠️ Claude exited with code $CLAUDE_EXIT_CODE: $(date)" >> "$LOG_FILE"
fi
echo "Exit Code: $CLAUDE_EXIT_CODE" >> "$LOG_FILE"
echo "========================================" >> "$LOG_FILE"

exit $CLAUDE_EXIT_CODE
