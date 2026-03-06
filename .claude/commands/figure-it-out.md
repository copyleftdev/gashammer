---
description: Crash recovery — analyze repo state, GitHub issues, and build status to determine exactly where we left off and what to do next
---

Something went wrong or we lost context. Figure out exactly where we left off and what needs to happen next.

This is the process-of-elimination solver. Run every diagnostic, cross-reference everything, and produce a clear action plan.

## Phase 1: Repo State Analysis

// turbo
1. Check working tree cleanliness:
   `git status --porcelain`
   If output is non-empty, we have a dirty repo. Catalog every dirty file. This is a VIOLATION of clean repo policy and must be resolved first.

// turbo
2. Identify current branch and its relationship to gitflow:
   `git branch --show-current && git log --oneline -5`
   Determine: are we on a feature branch? develop? main? A detached HEAD?

// turbo
3. Check for any uncommitted stashes:
   `git stash list`

// turbo
4. Check for any in-progress merges or rebases:
   `ls .git/MERGE_HEAD .git/REBASE_HEAD .git/rebase-merge .git/rebase-apply 2>/dev/null; echo "exit:$?"`

// turbo
5. Check relationship between local and remote:
   `git fetch origin 2>/dev/null; git status -sb`

## Phase 2: Build State Analysis

// turbo
6. Can the project compile?
   `cargo check --workspace 2>&1 | tail -30`
   If this fails, we have broken code. Identify what's broken.

// turbo
7. Do tests pass?
   `cargo test --workspace 2>&1 | tail -50`
   If tests fail, catalog which tests and in which crates.

// turbo
8. Any lint violations?
   `cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -30`

// turbo
9. Format check:
   `cargo fmt --all -- --check 2>&1 | tail -20`

## Phase 3: GitHub Issues Cross-Reference

// turbo
10. List all open issues:
    `gh issue list --state open --limit 50 --json number,title,labels,assignees,milestone,state,body 2>&1`
    If `gh` is not installed or not authenticated, note this and skip to Phase 4.

// turbo
11. List all open PRs:
    `gh pr list --state open --limit 20 --json number,title,headRefName,baseRefName,state,body 2>&1`

// turbo
12. Check if current branch has an associated PR:
    `gh pr view --json number,title,state,body 2>&1`

// turbo
13. List recent closed issues for context on what was just completed:
    `gh issue list --state closed --limit 10 --json number,title,closedAt --jq 'sort_by(.closedAt) | reverse' 2>&1`

## Phase 4: File System Forensics

// turbo
14. Check for any progress notes or TODO files:
    `find . -maxdepth 2 -name "progress.txt" -o -name "TODO.md" -o -name "PROGRESS.md" -o -name ".todo" 2>/dev/null`

// turbo
15. What files were most recently modified?
    `git diff --stat HEAD~5..HEAD 2>/dev/null || echo "not enough commits"`

// turbo
16. Check which crates exist vs which are expected (per CLAUDE.md):
    `ls -d crates/*/ 2>/dev/null || echo "no crates directory yet"`

// turbo
17. Check RFC status — which RFCs exist:
    `ls rfcs/*.md 2>/dev/null | sort`

## Phase 5: Synthesis and Action Plan

18. Based on ALL of the above, produce a structured recovery report:

    **RECOVERY REPORT**

    **Repo Health:**
    - Working tree: clean / dirty (list files)
    - Current branch: <name> (correct gitflow position: yes/no)
    - Unpushed commits: yes/no
    - Stashes: yes/no
    - In-progress merge/rebase: yes/no

    **Build Health:**
    - Compiles: yes/no (errors if no)
    - Tests: pass/fail (which crates)
    - Clippy: clean/violations
    - Format: clean/violations

    **GitHub State:**
    - Open issues: list with numbers and titles
    - Open PRs: list with branches
    - Current branch issue: #<number> or NONE (violation!)

    **Last Known Work:**
    - Most recent commits describe: <summary>
    - Most recently modified crates: <list>
    - Recently closed issues: <list>

    **IMMEDIATE ACTIONS (ordered):**
    1. [First thing to fix — dirty repo? broken build? orphan branch?]
    2. [Second priority]
    3. [Continue from here]

    **NEXT ISSUE TO WORK ON:**
    - Issue #<number>: <title>
    - Branch to create: `feature/GH-<number>-<description>`
    - What needs to happen: <concrete steps>

Always resolve repo cleanliness issues FIRST before any feature work.
