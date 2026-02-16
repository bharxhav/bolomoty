# bolo

A CLI tool that parses codebases using tree-sitter and emits a dependency DAG as JSON.

## Installation

TBA

## Usage

```bash
bolo <LANG> [PATH] [OPTIONS]
```

### Languages

| Alias | Language |
| ----- | -------- |
| `py`  | Python   |
| `rs`  | Rust     |

### Options

| Flag                | Description                           |
| ------------------- | ------------------------------------- |
| `-o <PATH>`         | Output directory (default: stdout)    |
| `-f, --force`       | Overwrite existing output             |
| `--no-ignore`       | Include files ignored by `.gitignore` |
| `-m, --mode <MODE>` | Limit output to a specific DAG mode   |

### Examples

```bash
bolo py .                        # parse current dir, print JSON
bolo py src/app.py               # parse a single file
bolo rs src/ -o out/             # save results to out/
bolo py . -o out/ -f             # overwrite existing output
bolo py . --no-ignore            # include gitignored files
bolo py . -m imports             # only emit import edges
```
