#!/usr/bin/env python3
"""
Run mcc-gaql-gen generate for all cookbook entries.
Writes intermediate results to reports/gen_results.<timestamp>/
"""

import argparse
import json
import re
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime
from pathlib import Path
from typing import Dict, List, Optional

import toml


def parse_generated_query(stdout: str) -> str:
    """Extract the generated GAQL query from stdout."""
    # Look for query in code blocks or after specific markers
    lines = stdout.split('\n')
    query_lines = []
    in_query = False

    for line in lines:
        stripped = line.strip()
        # Check for SQL/GAQL code block
        if stripped.startswith('```sql') or stripped.startswith('```gaql'):
            in_query = True
            continue
        elif stripped == '```' and in_query:
            in_query = False
            continue
        elif stripped.startswith('```') and in_query:
            in_query = False
            continue

        if in_query:
            query_lines.append(line)

    # If no code block found, try to find query after specific headers
    if not query_lines:
        for i, line in enumerate(lines):
            if 'Generated GAQL:' in line or 'Generated Query:' in line:
                # Look for next code block or indented SQL
                for j in range(i + 1, len(lines)):
                    if lines[j].strip().startswith('```'):
                        for k in range(j + 1, len(lines)):
                            if lines[k].strip() == '```':
                                break
                            query_lines.append(lines[k])
                        break
                    elif lines[j].strip().upper().startswith('SELECT'):
                        for k in range(j, len(lines)):
                            if lines[k].strip() and not lines[k].strip().startswith('#'):
                                query_lines.append(lines[k])
                            elif not lines[k].strip():
                                break
                        break
                break

    return '\n'.join(query_lines).strip()


def parse_explanation(stdout: str) -> str:
    """Extract the explanation from stdout."""
    # Look for explanation section
    lines = stdout.split('\n')
    explanation_lines = []
    in_explanation = False

    for i, line in enumerate(lines):
        stripped = line.strip()
        if 'Explanation:' in stripped or 'Reasoning:' in stripped:
            in_explanation = True
            # Don't include the header line itself
            continue
        elif in_explanation:
            # Stop if we hit a major section boundary or code block
            if stripped.startswith('```') or stripped.startswith('Generated GAQL:'):
                break
            explanation_lines.append(line)

    return '\n'.join(explanation_lines).strip()


def run_generation(entry_name: str, description: str, reference_query: str,
                 mcc_gaql_gen_path: str = "mcc-gaql-gen") -> dict:
    """Run mcc-gaql-gen generate and return result."""
    cmd = [
        mcc_gaql_gen_path, "generate", description,
        "--use-query-cookbook", "--explain", "--no-defaults"
    ]

    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=120  # 2 minute timeout per entry
        )

        stdout = result.stdout or ""
        stderr = result.stderr or ""

        return {
            "entry_name": entry_name,
            "description": description,
            "reference_query": reference_query,
            "generated_query": parse_generated_query(stdout),
            "explanation": parse_explanation(stdout),
            "full_stdout": stdout,
            "full_stderr": stderr,
            "status": "success" if result.returncode == 0 else "error",
            "returncode": result.returncode
        }
    except subprocess.TimeoutExpired:
        return {
            "entry_name": entry_name,
            "description": description,
            "reference_query": reference_query,
            "generated_query": "",
            "explanation": "",
            "full_stdout": "",
            "full_stderr": "Timeout after 120 seconds",
            "status": "timeout",
            "returncode": -1
        }
    except Exception as e:
        return {
            "entry_name": entry_name,
            "description": description,
            "reference_query": reference_query,
            "generated_query": "",
            "explanation": "",
            "full_stdout": "",
            "full_stderr": f"Exception: {str(e)}",
            "status": "exception",
            "returncode": -1
        }


def load_cookbook(cookbook_path: Path) -> List[Dict]:
    """Load cookbook and return list of entries."""
    with open(cookbook_path, 'r') as f:
        data = toml.load(f)

    entries = []
    for key, value in data.items():
        # Skip metadata sections
        if key in ('metadata', 'version'):
            continue
        if isinstance(value, dict) and 'description' in value and 'query' in value:
            entries.append({
                'name': key,
                'description': value['description'].strip(),
                'query': value['query'].strip()
            })

    return entries


def get_default_cookbook_path() -> Optional[Path]:
    """Get default cookbook path based on platform."""
    home = Path.home()

    # Linux path
    linux_path = home / '.config' / 'mcc-gaql' / 'query_cookbook.toml'
    if linux_path.exists():
        return linux_path

    # macOS path
    macos_path = home / 'Library' / 'Application Support' / 'mcc-gaql' / 'query_cookbook.toml'
    if macos_path.exists():
        return macos_path

    return None


def main():
    parser = argparse.ArgumentParser(
        description='Run mcc-gaql-gen generate for all cookbook entries'
    )
    parser.add_argument(
        '--cookbook', '-c',
        type=Path,
        help='Path to query_cookbook.toml (default: auto-discover)'
    )
    parser.add_argument(
        '--output', '-o',
        type=Path,
        default=None,
        help='Output directory (default: reports/gen_results.<timestamp>)'
    )
    parser.add_argument(
        '--workers', '-w',
        type=int,
        default=5,
        help='Number of concurrent workers (default: 5)'
    )
    parser.add_argument(
        '--mcc-gaql-gen',
        default='mcc-gaql-gen',
        help='Path to mcc-gaql-gen binary (default: mcc-gaql-gen)'
    )
    parser.add_argument(
        '--entry',
        help='Run for a single entry only'
    )
    parser.add_argument(
        '--dry-run',
        action='store_true',
        help='Print what would be done without running'
    )

    args = parser.parse_args()

    # Determine cookbook path
    if args.cookbook:
        cookbook_path = args.cookbook
    else:
        cookbook_path = get_default_cookbook_path()
        if not cookbook_path:
            print("Error: Could not find query_cookbook.toml. Specify with --cookbook", file=sys.stderr)
            sys.exit(1)

    if not cookbook_path.exists():
        print(f"Error: Cookbook not found: {cookbook_path}", file=sys.stderr)
        sys.exit(1)

    print(f"Loading cookbook from: {cookbook_path}")
    entries = load_cookbook(cookbook_path)
    print(f"Found {len(entries)} entries")

    # Filter to single entry if specified
    if args.entry:
        entries = [e for e in entries if e['name'] == args.entry]
        if not entries:
            print(f"Error: Entry '{args.entry}' not found", file=sys.stderr)
            sys.exit(1)
        print(f"Running single entry: {args.entry}")

    # Create output directory
    if args.output:
        output_dir = args.output
    else:
        timestamp = datetime.now().strftime('%Y%m%d%H%M%S')
        output_dir = Path('reports') / f'gen_results.{timestamp}'

    if args.dry_run:
        print(f"Would create output directory: {output_dir}")
        print(f"Would process {len(entries)} entries with {args.workers} workers")
        for entry in entries[:3]:
            print(f"  - {entry['name']}")
        if len(entries) > 3:
            print(f"  ... and {len(entries) - 3} more")
        sys.exit(0)

    output_dir.mkdir(parents=True, exist_ok=True)
    print(f"Output directory: {output_dir}")

    # Run generations with concurrency control
    completed = 0
    failed = 0

    with ThreadPoolExecutor(max_workers=args.workers) as executor:
        # Submit all tasks
        future_to_entry = {
            executor.submit(
                run_generation,
                entry['name'],
                entry['description'],
                entry['query'],
                args.mcc_gaql_gen
            ): entry
            for entry in entries
        }

        # Process results as they complete
        for future in as_completed(future_to_entry):
            entry = future_to_entry[future]
            try:
                result = future.result()

                # Save result to JSON file
                output_file = output_dir / f"{result['entry_name']}.json"
                with open(output_file, 'w') as f:
                    json.dump(result, f, indent=2)

                if result['status'] == 'success':
                    completed += 1
                    print(f"  {completed}/{len(entries)}: {result['entry_name']} - OK")
                else:
                    failed += 1
                    print(f"  {completed + failed}/{len(entries)}: {result['entry_name']} - {result['status'].upper()}")

            except Exception as e:
                failed += 1
                print(f"  {completed + failed}/{len(entries)}: {entry['name']} - EXCEPTION: {e}")

    print(f"\nResults saved to: {output_dir}")
    print(f"Completed: {completed}, Failed: {failed}, Total: {len(entries)}")

    # Write summary file
    summary = {
        'timestamp': datetime.now().isoformat(),
        'cookbook_path': str(cookbook_path),
        'total_entries': len(entries),
        'completed': completed,
        'failed': failed,
        'output_directory': str(output_dir)
    }
    summary_file = output_dir / '_summary.json'
    with open(summary_file, 'w') as f:
        json.dump(summary, f, indent=2)

    print(f"Summary written to: {summary_file}")

    return 0 if failed == 0 else 1


if __name__ == '__main__':
    sys.exit(main())
