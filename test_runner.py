#!/usr/bin/env python3
"""
Automated test runner for mcc-gaql using Claude CLI.
Replaces run_tests.sh with robust error handling and logging.
"""

import subprocess
import sys
import os
from pathlib import Path
from datetime import datetime
import json

# Configuration
PROJECT_DIR = Path.home() / "Development/googleads/mcc-gaql"
LOG_DIR = PROJECT_DIR / "logs"
NOTIFICATION_FILE = LOG_DIR / ".notification_pending"
# Timeout for Claude CLI execution (in seconds)
CLAUDE_TIMEOUT = 600  # 10 minutes max

# Use full path to Claude CLI to avoid PATH issues
CLAUDE_BIN = os.environ.get("CLAUDE_BIN", Path.home() / ".local/bin/claude")

# Use full path to Claude CLI to avoid PATH issues
CLAUDE_BIN = os.environ.get("CLAUDE_BIN", Path.home() / ".local/bin/claude")

CLAUDE_CMD = [
    str(CLAUDE_BIN), "-p",
    "Run all tests in this Rust workspace using 'cargo test --workspace --no-fail-fast'. After tests complete (success or failure), report the summary and then exit immediately. Do not wait for further input.",
    "--dangerously-skip-permissions",
    "--model", "haiku",
    "--no-session-persistence"  # Don't persist session, exit when done
]


def ensure_directories():
    """Create logs directory if it doesn't exist."""
    LOG_DIR.mkdir(parents=True, exist_ok=True)


def get_timestamp() -> str:
    """Generate timestamp for filenames."""
    return datetime.now().strftime("%Y%m%d_%H%M%S")


def write_log_header(log_file: Path, timestamp: str):
    """Write header to log file."""
    with open(log_file, "w") as f:
        f.write("=" * 50 + "\n")
        f.write(f"Claude Test Run Started: {datetime.now().isoformat()}\n")
        f.write(f"Working Directory: {PROJECT_DIR}\n")
        f.write(f"Timestamp: {timestamp}\n")
        f.write("=" * 50 + "\n\n")


def write_log_footer(log_file: Path, exit_code: int):
    """Write footer to log file."""
    with open(log_file, "a") as f:
        f.write("\n" + "=" * 50 + "\n")
        status = "✅ PASSED" if exit_code == 0 else f"⚠️ FAILED (exit {exit_code})"
        f.write(f"Test Status: {status}\n")
        f.write(f"Completed: {datetime.now().isoformat()}\n")
        f.write("=" * 50 + "\n")


def run_tests(log_file: Path) -> int:
    """Run Claude CLI tests and capture output. Returns exit code."""
    try:
        with open(log_file, "a") as f:
            f.write("Invoking Claude CLI to run tests...\n")
            f.write(f"Command: {' '.join(CLAUDE_CMD)}\n")
            f.write(f"Timeout: {CLAUDE_TIMEOUT}s\n\n")
            f.flush()
            
            # Run claude with timeout
            process = subprocess.Popen(
                CLAUDE_CMD,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                cwd=PROJECT_DIR,
                text=True,
                bufsize=1
            )
            
            # Stream output with timeout
            output_lines = []
            start_time = datetime.now()
            
            try:
                for line in process.stdout:
                    f.write(line)
                    f.flush()
                    output_lines.append(line)
                    
                    # Check timeout
                    elapsed = (datetime.now() - start_time).total_seconds()
                    if elapsed > CLAUDE_TIMEOUT:
                        f.write(f"\n⚠️ TIMEOUT: Process exceeded {CLAUDE_TIMEOUT}s, terminating...\n")
                        process.terminate()
                        try:
                            process.wait(timeout=10)
                        except subprocess.TimeoutExpired:
                            process.kill()
                            process.wait()
                        return 124  # Standard timeout exit code
                        
            except Exception as e:
                f.write(f"\n⚠️ Error reading output: {e}\n")
                process.kill()
                return 1
            
            process.wait()
            return process.returncode
            
    except FileNotFoundError:
        with open(log_file, "a") as f:
            f.write("\n❌ ERROR: Claude CLI not found in PATH\n")
            f.write(f"PATH: {os.environ.get('PATH', 'Not set')}\n")
        return 127  # Command not found
        
    except Exception as e:
        with open(log_file, "a") as f:
            f.write(f"\n❌ ERROR: {type(e).__name__}: {e}\n")
        return 1


def create_notification(timestamp: str, exit_code: int, log_file: Path):
    """Create notification file for the notifier to pick up."""
    notification = {
        "test_run": timestamp,
        "status": "PASSED" if exit_code == 0 else "FAILED",
        "exit_code": exit_code,
        "log_file": str(log_file),
        "time": datetime.now().isoformat(),
        "project": "mcc-gaql"
    }
    
    # Write JSON format for easier parsing
    with open(NOTIFICATION_FILE, "w") as f:
        json.dump(notification, f, indent=2)
    
    # Also write human-readable format
    with open(NOTIFICATION_FILE, "a") as f:
        f.write("\n---\n")
        f.write(f"Test Run: {timestamp}\n")
        f.write(f"Status: {notification['status']}\n")
        f.write(f"Exit Code: {exit_code}\n")
        f.write(f"Log File: {log_file}\n")
        f.write(f"Time: {notification['time']}\n")
        f.write(f"Project: mcc-gaql\n")


def main():
    """Main entry point."""
    ensure_directories()
    
    timestamp = get_timestamp()
    log_file = LOG_DIR / f"claude_test_run_{timestamp}.log"
    
    write_log_header(log_file, timestamp)
    
    # Run tests (this will always complete, even on failure)
    exit_code = run_tests(log_file)
    
    write_log_footer(log_file, exit_code)
    
    # Always create notification, regardless of test outcome
    create_notification(timestamp, exit_code, log_file)
    
    # Return exit code for cron job reporting
    return exit_code


if __name__ == "__main__":
    sys.exit(main())
