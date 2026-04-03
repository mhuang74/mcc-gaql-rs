#!/usr/bin/env python3
"""
Run mcc-gaql-gen generate for all cookbook entries.
Writes intermediate results to tmp/gen_results.<timestamp>/
"""

import argparse
import json
import random
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
    # The query appears at the start of stdout, followed by explanation section
    # Look for the RAG SELECTION EXPLANATION header or box-drawing characters
    lines = stdout.split('\n')
    query_lines = []

    for line in lines:
        stripped = line.strip()
        # Stop when we hit the explanation section
        if 'RAG SELECTION EXPLANATION' in stripped:
            break
        if stripped and stripped[0] == '\u2550':  # Box-drawing character ═
            break
        # Also stop at common explanation headers
        if stripped in ('Explanation:', 'Reasoning:', 'Selected Resource:',
                        '## Phase 1:', '## Phase 2:', '## Phase 3:', '## Phase 4:'):
            break
        query_lines.append(line)

    # Strip trailing empty lines
    while query_lines and not query_lines[-1].strip():
        query_lines.pop()

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


def parse_markdown_result(content: str) -> dict:
    """Parse a Markdown result file back into a dict."""
    result = {}

    # Extract entry name from title
    title_match = re.search(r'^# Query Generation Result: (.+)$', content, re.MULTILINE)
    result['entry_name'] = title_match.group(1) if title_match else ''

    # Extract description
    desc_match = re.search(r'## Description\n(.+?)\n\n##', content, re.DOTALL)
    result['description'] = desc_match.group(1).strip() if desc_match else ''

    # Extract reference query
    ref_match = re.search(r'## Reference Query\n```sql\n(.+?)\n```', content, re.DOTALL)
    result['reference_query'] = ref_match.group(1).strip() if ref_match else ''

    # Extract generated query
    gen_match = re.search(r'## Generated Query\n```sql\n(.+?)\n```', content, re.DOTALL)
    result['generated_query'] = gen_match.group(1).strip() if gen_match else ''

    # Extract status
    status_match = re.search(r'## Status: (\w+)', content)
    result['status'] = status_match.group(1).lower() if status_match else 'unknown'

    # Extract returncode
    rc_match = re.search(r'returncode: (-?\d+)', content)
    result['returncode'] = int(rc_match.group(1)) if rc_match else -1

    # Extract explanation (everything between LLM Explanation and Full Output)
    expl_match = re.search(r'## LLM Explanation\n(.+?)\n\n## Full Output', content, re.DOTALL)
    result['explanation'] = expl_match.group(1).strip() if expl_match else ''

    # Extract stdout
    stdout_match = re.search(r'### Stdout\n```\n(.+?)\n```', content, re.DOTALL)
    result['full_stdout'] = stdout_match.group(1).strip() if stdout_match else ''

    # Extract stderr
    stderr_match = re.search(r'### Stderr\n```\n(.+?)\n```', content, re.DOTALL)
    result['full_stderr'] = stderr_match.group(1).strip() if stderr_match else ''

    return result


def format_markdown_result(result: dict) -> str:
    """Format result as human-readable Markdown."""
    lines = [
        f"# Query Generation Result: {result['entry_name']}",
        "",
        "## Description",
        result['description'],
        "",
        "## Reference Query",
        "```sql",
        result['reference_query'],
        "```",
        "",
        "## Generated Query",
        "```sql",
        result['generated_query'] if result['generated_query'] else "-- No query generated",
        "```",
        "",
        f"## Status: {result['status'].upper()} (returncode: {result['returncode']})",
        "",
        "## LLM Explanation",
        result['explanation'] if result['explanation'] else "_No explanation provided_",
        "",
        "## Full Output",
        "",
        "### Stdout",
        "```",
        result['full_stdout'] if result['full_stdout'] else "_No output_",
        "```",
        "",
    ]

    if result['full_stderr']:
        lines.extend([
            "### Stderr",
            "```",
            result['full_stderr'],
            "```",
            "",
        ])

    return '\n'.join(lines)


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
        help='Output directory (default: tmp/gen_results.<timestamp>)'
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
        '--random', '-r',
        type=int,
        metavar='N',
        help='Randomly select N entries to test'
    )
    parser.add_argument(
        '--seed',
        type=int,
        help='Random seed for reproducible selection (use with --random)'
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

    # Randomly sample N entries if specified
    if args.random:
        if args.random > len(entries):
            print(f"Warning: Requested {args.random} entries but only {len(entries)} available", file=sys.stderr)
            args.random = len(entries)
        if args.seed is not None:
            random.seed(args.seed)
            print(f"Random seed: {args.seed}")
        entries = random.sample(entries, args.random)
        print(f"Randomly selected {len(entries)} entries")

    # Create output directory
    if args.output:
        output_dir = args.output
    else:
        timestamp = datetime.now().strftime('%Y%m%d%H%M%S')
        output_dir = Path('tmp') / f'gen_results.{timestamp}'

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

                # Save result to Markdown file
                output_file = output_dir / f"{result['entry_name']}.md"
                with open(output_file, 'w', encoding='utf-8') as f:
                    f.write(format_markdown_result(result))

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

    # Write summary as Markdown
    summary_lines = [
        "# Query Cookbook Generation Test Summary",
        "",
        f"**Timestamp:** {datetime.now().isoformat()}",
        f"**Cookbook:** {cookbook_path}",
        f"**Output Directory:** {output_dir}",
        "",
        "## Statistics",
        "",
        f"- **Total Entries:** {len(entries)}",
        f"- **Completed:** {completed}",
        f"- **Failed:** {failed}",
        "",
        "## Result Files",
        "",
    ]

    # List all result files
    for md_file in sorted(output_dir.glob('*.md')):
        if md_file.name.startswith('_'):
            continue
        summary_lines.append(f"- [{md_file.stem}]({md_file.name})")

    if failed > 0:
        summary_lines.extend([
            "",
            "## Failed Entries",
            "",
        ])
        for md_file in sorted(output_dir.glob('*.md')):
            if md_file.name.startswith('_'):
                continue
            # Quick check for failed status
            content = md_file.read_text()
            if '## Status: ERROR' in content or '## Status: TIMEOUT' in content or '## Status: EXCEPTION' in content:
                summary_lines.append(f"- [{md_file.stem}]({md_file.name})")

    summary_file = output_dir / '_summary.md'
    with open(summary_file, 'w', encoding='utf-8') as f:
        f.write('\n'.join(summary_lines))

    print(f"Summary written to: {summary_file}")

    return 0 if failed == 0 else 1


if __name__ == '__main__':
    sys.exit(main())
