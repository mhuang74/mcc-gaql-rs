#!/bin/bash
# Send test results notification via Telegram
# Usage: send_notification.sh <channel> <chat_id>

CHANNEL="${1:-telegram}"
CHAT_ID="${2:-8281248569}"
LOGS_DIR="$HOME/Development/googleads/mcc-gaql/logs"
NOTIFICATION_FILE="$LOGS_DIR/.notification_pending"

# Check for notification file created by run_tests.sh
if [ -f "$NOTIFICATION_FILE" ] && [ -s "$NOTIFICATION_FILE" ]; then
    # Read the notification content
    SUMMARY=$(cat "$NOTIFICATION_FILE")
    
    # Also include recent log summary if available
    LATEST_LOG=$(ls -t "$LOGS_DIR"/claude_test_run_*.log 2>/dev/null | head -1)
    if [ -n "$LATEST_LOG" ]; then
        LOG_TAIL=$(tail -20 "$LATEST_LOG" 2>/dev/null)
        SUMMARY="${SUMMARY}

--- Recent Log Output ---
${LOG_TAIL}"
    fi
    
    # Create JSON payload
    JSON_PAYLOAD=$(jq -n \
        --arg channel "$CHANNEL" \
        --arg chat_id "$CHAT_ID" \
        --arg content "$SUMMARY" \
        '{type: "message", channel: $channel, chat_id: $chat_id, content: $content}')
    
    # Send via nanobot gateway if available
    if curl -s -o /dev/null -w "%{http_code}" http://localhost:18790/message \
            -H "Content-Type: application/json" \
            -d "$JSON_PAYLOAD" 2>/dev/null | grep -q "200"; then
        echo "✅ Notification sent via gateway at $(date)"
        # Move notification file to sent status
        mv "$NOTIFICATION_FILE" "${NOTIFICATION_FILE}.sent.$(date +%s)"
    else
        # Fallback: keep the notification file for retry
        echo "⚠️ Gateway unavailable at $(date), notification will retry"
    fi
else
    # Also check for old-style summary files for backward compatibility
    LATEST_SUMMARY=$(ls -t "$LOGS_DIR"/notification_summary_*.txt 2>/dev/null | head -1)
    if [ -n "$LATEST_SUMMARY" ]; then
        SUMMARY=$(cat "$LATEST_SUMMARY")
        
        # Create JSON payload
        JSON_PAYLOAD=$(jq -n \
            --arg channel "$CHANNEL" \
            --arg chat_id "$CHAT_ID" \
            --arg content "$SUMMARY" \
            '{type: "message", channel: $channel, chat_id: $chat_id, content: $content}')
        
        # Send via nanobot gateway if available
        if curl -s -o /dev/null -w "%{http_code}" http://localhost:18790/message \
                -H "Content-Type: application/json" \
                -d "$JSON_PAYLOAD" 2>/dev/null | grep -q "200"; then
            echo "✅ Notification sent via gateway at $(date)"
            mv "$LATEST_SUMMARY" "${LATEST_SUMMARY}.sent"
        else
            echo "⚠️ Gateway unavailable, message queued at $(date)"
        fi
    else
        echo "No pending notifications at $(date)"
    fi
fi
