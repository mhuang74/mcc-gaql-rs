#!/usr/bin/env python3
"""
Dry-run test for test_runner.py
Simulates the test execution without actually running claude.
"""

import subprocess
import sys
import os
from pathlib import Path
from datetime import datetime
import json

# Configuration (same as test_runner.py)
PROJECT_DIR = Path.home() / "Development/googleads/mcc-gaql"
LOG_DIR = PROJECT_DIR / "logs"
NOTIFICATION_FILE = LOG_DIR / ".notification_pending"

# Simulate test failure/success
SIMULATE_EXIT_CODE = 1  # Change to 0 to test success path


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
        f.write(f"DRY RUN - Claude Test Run Started: {datetime.now().isoformat()}\n")
        f.write(f"Working Directory: {PROJECT_DIR}\n")
        f.write(f"Timestamp: {timestamp}\n")
        f.write("=" * 50 + "\n\n")


def write_log_footer(log_file: Path, exit_code: int):
    """Write footer to log file."""
    with open(log_file, "a") as f:
        f.write("\n" + "=" * 50 + "\n")
        status = "✅ PASSED" if exit_code == 0 else f"⚠️ FAILED (exit {exit_code})"
        f.write(f"DRY RUN - Test Status: {status}\n")
        f.write(f"Completed: {datetime.now().isoformat()}\n")
        f.write("=" * 50 + "\n")


def run_simulated_tests(log_file: Path) -> int:
    """Simulate test execution (no actual claude call)."""
    with open(log_file, "a") as f:
        f.write("DRY RUN: Simulating Claude CLI execution...\n")
        f.write("This is a dry run - no actual tests were executed\n")
        f.write(f"Simulated exit code: {SIMULATE_EXIT_CODE}\n")
        f.flush()
    
    # Return simulated exit code
    return SIMULATE_EXIT_CODE


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
    print("🧪 DRY RUN: Testing test_runner.py logic...")
    print(f"Project directory: {PROJECT_DIR}")
    print(f"Log directory: {LOG_DIR}")
    print(f"Notification file: {NOTIFICATION_FILE}")
    print(f"Simulated exit code: {SIMULATE_EXIT_CODE}")
    print()
    
    ensure_directories()
    
    timestamp = get_timestamp()
    log_file = LOG_DIR / f"claude_test_run_{timestamp}.log"
    
    print(f"Creating log file: {log_file}")
    write_log_header(log_file, timestamp)
    
    # Run simulated tests (this will always complete, even on simulated failure)
    print("Running simulated tests...")
    exit_code = run_simulated_tests(log_file)
    
    write_log_footer(log_file, exit_code)
    
    # Always create notification, regardless of test outcome
    print("Creating notification file...")
    create_notification(timestamp, exit_code, log_file)
    
    print()
    print("=" * 50)
    print("DRY RUN COMPLETE")
    print("=" * 50)
    print(f"Log file created: {log_file}")
    print(f"Notification file: {NOTIFICATION_FILE}")
    print()
    print("Notification content:")
    print(NOTIFICATION_FILE.read_text())
    
    return exit_code


if __name__ == "__main__":
    sys.exit(main())
