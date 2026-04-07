#!/usr/bin/env python3
"""
Simple test runner - runs cargo test directly without Claude CLI.
More reliable for automated cron jobs.
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
CARGO_TIMEOUT = 600  # 10 minutes


def ensure_directories():
    """Create logs directory if it doesn't exist."""
    LOG_DIR.mkdir(parents=True, exist_ok=True)


def get_timestamp() -> str:
    """Generate timestamp for filenames."""
    return datetime.now().strftime("%Y%m%d_%H%M%S")


def run_tests_direct(log_file: Path) -> int:
    """Run cargo test directly without Claude CLI."""
    try:
        with open(log_file, "w") as f:
            f.write("=" * 50 + "\n")
            f.write(f"Direct Test Run Started: {datetime.now().isoformat()}\n")
            f.write(f"Working Directory: {PROJECT_DIR}\n")
            f.write(f"Command: cargo test --workspace --no-fail-fast\n")
            f.write(f"Timeout: {CARGO_TIMEOUT}s\n")
            f.write("=" * 50 + "\n\n")
            f.flush()
            
            # Run cargo test with timeout
            process = subprocess.Popen(
                ["cargo", "test", "--workspace", "--no-fail-fast"],
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                cwd=PROJECT_DIR,
                text=True,
                bufsize=1
            )
            
            # Stream output
            start_time = datetime.now()
            try:
                for line in process.stdout:
                    f.write(line)
                    f.flush()
                    
                    # Check timeout
                    elapsed = (datetime.now() - start_time).total_seconds()
                    if elapsed > CARGO_TIMEOUT:
                        f.write(f"\n⚠️ TIMEOUT: Tests exceeded {CARGO_TIMEOUT}s\n")
                        process.kill()
                        return 124
                        
            except Exception as e:
                f.write(f"\n⚠️ Error: {e}\n")
                process.kill()
                return 1
            
            process.wait()
            
            # Write footer
            f.write("\n" + "=" * 50 + "\n")
            status = "✅ PASSED" if process.returncode == 0 else f"⚠️ FAILED (exit {process.returncode})"
            f.write(f"Test Status: {status}\n")
            f.write(f"Completed: {datetime.now().isoformat()}\n")
            f.write("=" * 50 + "\n")
            
            return process.returncode
            
    except FileNotFoundError:
        with open(log_file, "a") as f:
            f.write("\n❌ ERROR: cargo not found in PATH\n")
        return 127
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
        "project": "mcc-gaql",
        "runner": "cargo-direct"
    }
    
    with open(NOTIFICATION_FILE, "w") as f:
        json.dump(notification, f, indent=2)
        f.write("\n---\n")
        f.write(f"Test Run: {timestamp}\n")
        f.write(f"Status: {notification['status']}\n")
        f.write(f"Exit Code: {exit_code}\n")
        f.write(f"Log File: {log_file}\n")


def main():
    """Main entry point."""
    ensure_directories()
    
    timestamp = get_timestamp()
    log_file = LOG_DIR / f"cargo_test_run_{timestamp}.log"
    
    # Run tests directly
    exit_code = run_tests_direct(log_file)
    
    # Always create notification
    create_notification(timestamp, exit_code, log_file)
    
    return exit_code


if __name__ == "__main__":
    sys.exit(main())
