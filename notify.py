#!/usr/bin/env python3
"""
Notification sender for mcc-gaql test results.
Uses Telegram Bot API directly (avoids gateway crashes).
"""

import json
import os
import sys
import subprocess
import urllib.request
import urllib.error
from pathlib import Path
from datetime import datetime
import time

# Configuration
PROJECT_DIR = Path.home() / "Development/googleads/mcc-gaql"
LOG_DIR = PROJECT_DIR / "logs"
NOTIFICATION_FILE = LOG_DIR / ".notification_pending"

# Default notification target (can be overridden via env vars or args)
DEFAULT_CHANNEL = os.environ.get("NOTIFY_CHANNEL", "telegram")
DEFAULT_CHAT_ID = os.environ.get("NOTIFY_CHAT_ID", "8700663659")

# Telegram Bot Token (read from nanobot config)
TELEGRAM_BOT_TOKEN = "8681150307:AAF60g-7DXEje4_qOjKUg2Mgd6WP-uLkxFI"
TELEGRAM_API_URL = f"https://api.telegram.org/bot{TELEGRAM_BOT_TOKEN}/sendMessage"


def find_latest_log() -> Path | None:
    """Find the most recent claude test log file."""
    log_files = sorted(LOG_DIR.glob("claude_test_run_*.log"), reverse=True)
    return log_files[0] if log_files else None


def read_notification() -> dict | None:
    """Read notification file if it exists."""
    if NOTIFICATION_FILE.exists() and NOTIFICATION_FILE.stat().st_size > 0:
        try:
            # Try to parse JSON first
            content = NOTIFICATION_FILE.read_text()
            if content.startswith('{'):
                return json.loads(content)
            # Fallback: parse text format
            return parse_text_notification(content)
        except json.JSONDecodeError:
            # Try text format
            return parse_text_notification(NOTIFICATION_FILE.read_text())
        except Exception as e:
            print(f"Error reading notification file: {e}")
            return None
    return None


def parse_text_notification(content: str) -> dict:
    """Parse text format notification."""
    notification = {}
    for line in content.strip().split('\n'):
        if ':' in line and not line.startswith('---'):
            key, value = line.split(':', 1)
            notification[key.strip().lower().replace(' ', '_')] = value.strip()
    return notification


def get_log_tail(log_file: Path, lines: int = 20) -> str:
    """Get last N lines from log file."""
    try:
        with open(log_file, 'r') as f:
            all_lines = f.readlines()
            return ''.join(all_lines[-lines:])
    except Exception as e:
        return f"[Could not read log: {e}]"


def format_message(notification: dict) -> str:
    """Format notification as message text."""
    status = notification.get('status', 'UNKNOWN')
    exit_code = notification.get('exit_code', 'N/A')
    test_run = notification.get('test_run', 'unknown')
    log_file = notification.get('log_file', 'unknown')
    
    # Determine emoji based on status
    if status == 'PASSED':
        emoji = '✅'
    elif status == 'FAILED':
        emoji = '❌'
    else:
        emoji = '⚠️'
    
    message = f"""{emoji} **MCC-GAQL Test Run**

**Status:** {status} (exit code: {exit_code})
**Test Run:** {test_run}
**Time:** {datetime.now().isoformat()}
"""
    
    # Add log tail for failed runs
    if status == 'FAILED' and log_file and log_file != 'unknown':
        log_path = Path(log_file)
        if log_path.exists():
            tail = get_log_tail(log_path, 15)
            message += f"\n--- Recent Log Output ---\n```\n{tail}```"
    
    return message


def escape_markdown(text: str) -> str:
    """Escape special characters for Telegram MarkdownV2."""
    # Characters that need escaping in MarkdownV2: _ * [ ] ( ) ~ ` > # + - = | { } . !
    escape_chars = r'_*[]()~`>#+-=|{}.!'
    for char in escape_chars:
        text = text.replace(char, f'\\{char}')
    return text


def send_via_telegram(message: str, chat_id: str) -> bool:
    """Send notification via Telegram Bot API directly."""
    # Escape the message for MarkdownV2
    escaped_message = escape_markdown(message)
    
    payload = {
        "chat_id": chat_id,
        "text": escaped_message,
        "parse_mode": "MarkdownV2",
        "disable_web_page_preview": True
    }
    
    try:
        data = json.dumps(payload).encode('utf-8')
        req = urllib.request.Request(
            TELEGRAM_API_URL,
            data=data,
            headers={'Content-Type': 'application/json'},
            method='POST'
        )
        
        with urllib.request.urlopen(req, timeout=15) as response:
            result = json.loads(response.read().decode('utf-8'))
            if result.get('ok'):
                return True
            else:
                print(f"Telegram API error: {result.get('description', 'Unknown error')}")
                return False
                
    except urllib.error.HTTPError as e:
        print(f"HTTP Error {e.code}: {e.read().decode('utf-8')}")
        return False
    except urllib.error.URLError as e:
        print(f"URL Error: {e.reason}")
        return False
    except Exception as e:
        print(f"Failed to send via Telegram: {e}")
        return False


def archive_notification():
    """Move notification file to sent status."""
    timestamp = datetime.now().strftime("%s")
    archive_file = LOG_DIR / f".notification_pending.sent.{timestamp}"
    try:
        NOTIFICATION_FILE.rename(archive_file)
        return True
    except Exception as e:
        print(f"Failed to archive notification: {e}")
        return False


def send_notification(channel: str = DEFAULT_CHANNEL, chat_id: str = DEFAULT_CHAT_ID) -> bool:
    """Main notification logic."""
    notification = read_notification()
    
    if not notification:
        # Check for old-style summary files (backward compatibility)
        summary_files = sorted(LOG_DIR.glob("notification_summary_*.txt"))
        if summary_files:
            summary_content = summary_files[0].read_text()
            message = f"📋 **Legacy Notification**\n\n{summary_content}"
            if send_via_telegram(message, chat_id):
                summary_files[0].rename(str(summary_files[0]) + ".sent")
                print(f"✅ Legacy notification sent at {datetime.now().isoformat()}")
                return True
            else:
                print(f"⚠️ Telegram API failed at {datetime.now().isoformat()}")
                return False
        else:
            print(f"No pending notifications at {datetime.now().isoformat()}")
            return True
    
    # Format and send notification
    message = format_message(notification)
    
    if send_via_telegram(message, chat_id):
        archive_notification()
        print(f"✅ Notification sent via Telegram at {datetime.now().isoformat()}")
        return True
    else:
        print(f"⚠️ Telegram API failed at {datetime.now().isoformat()}, notification will retry")
        return False


def main():
    """Main entry point."""
    # Parse arguments
    channel = sys.argv[1] if len(sys.argv) > 1 else DEFAULT_CHANNEL
    chat_id = sys.argv[2] if len(sys.argv) > 2 else DEFAULT_CHAT_ID
    
    success = send_notification(channel, chat_id)
    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
