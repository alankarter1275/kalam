# Kalam handoff rules

One rule: **every fact lives in exactly one file, and every chat ends by handing back complete replacement files built from the project's copies, never from memory.** There are no "update" files.

Handoff rules v1 (decision 103). If the Instructions field and this file disagree, this file wins.

## 1. Files

| File | Holds | How it changes |
|---|---|---|
| `decisions.md` | Every design decision, numbered; open questions; "not designed yet" lists | A new section per chat, appended. Older entries change only by being superseded (section 4) |
| `design-system.md` | Tokens, type, shape, components, GTK constraints | An "Additions" block is appended **and** older text is edited in place so nothing contradicts the new block |
| `kalam-screens.md` | The master checklist of screens and surfaces | Ticks, notes and open questions are edited in place |
| `<screen>-spec.md` | The builder spec for one finished screen or group of screens | One file per screen group; later chats edit it in place |
| `logo-svg.md`, `splash.html` | Assets | Final |
| `HANDOFF.md` | These rules | Only through a numbered decision |

Rules for files:
- Never create files named after a chat, a version or "update". Never keep two copies of anything.
- Specs name tokens and cite sections of `design-system.md`. They do not restate colour values. Decisions are cited by number.
- A source URL for the boards goes in the spec header and in the chat's decisions section.

## 2. Start of a chat

1. Read `HANDOFF.md`, `decisions.md`, `design-system.md`, `kalam-screens.md`, and the specs of neighbouring screens.
2. Say which checklist items the chat covers.
3. If a project file disagrees with anything remembered, the project file wins. Ask at most one question at a time.
4. During the chat, note every explicit ruling by the user as it happens. They become decisions at handoff.

## 3. End of a chat (when the user asks for the handoff)

1. Copy the project files to a working folder. Work from these copies.
2. **decisions.md:** append the chat's section, numbered on from the last decision. Mark which items the user did not explicitly question as *Proposed*.
3. **design-system.md:** append an "Additions vX.Y" block (bump the minor version, and update the version in the title). Then edit older sections, TBD lists and component lists so they agree.
4. **kalam-screens.md:** tick only what the user approved. Use `[~]` for partly done, with a note of what is missing. Strike through replaced items, do not delete them. Update the open questions.
5. Write or revise the spec(s), using the template in section 5.
6. Run the check script (section 7). Fix every error and read every warning.
7. Deliver complete replacement files for the files that changed, plus a three-line list: upload (replace), upload (new), remove from the project. Compare the delivered files with the project copies to confirm.

Never deliver pasted snippets, partial files or update files.

## 4. Writing rules

**Decisions**
- Numbers run on across chats. A number is never reused or renumbered.
- Each chat section starts `# <Screens> (design chat N)`, then the boards URL and a status line. Write "approved by the user at the end of design chat N" only if they said so.
- Each decision is self-contained: what was decided, the key values, and *Proposed* where it applies.
- A new decision never silently changes an old one. Write "replaces N" or "supersedes part of N", and add "*Superseded by M.*" to the old entry.
- Open questions are listed at the end of the chat section. A resolved question is struck through where it was first asked, with the decision number.
- End each chat section with "Not designed yet" and the footer "Numbering continues from N."

**Design system**
- Sections are numbered on across versions. New tokens are named with their values in exactly one section.
- Each Additions block starts with its status note (Proposed unless stated; which palettes were checked).

**Checklist**
- `[x]` designed and approved. `[~]` partly designed. `[ ]` not done. `(R)` confirmed in the repo, `(?)` inferred.
- A screen is `[x]` only when the user has approved it, and it has a spec.

**Specs.** Title ends with "(draft)" until the user approves. Sections: a header line (source boards, tokens, decisions covered); 1 Layout; 2 Components; 3 Dialogs (if any); 4 Tokens used; 5 States, with Drawn and Not drawn; 6 GTK widgets and custom drawing; 7 Notes for the builder.

## 5. Changes of mind and conflicts

- A ruling by the user beats an earlier suggestion. Record it as a decision.
- When a later chat changes a screen from an earlier chat, edit that screen's spec in place and write a decision that names the old ones.
- Anything not shown in the boards stays under "Not designed yet". Never mark it done.
- Out-of-scope items stay out (for example tap-to-look-up, decision 9).

## 6. Boards (design canvases)

- One canvas per screen group. Light and dark pairs. A dark-only board is listed under "Not designed yet".
- A publish replaces a canvas's whole file set: always send every file. The limit is 16 files per call. If a canvas would pass that, merge states into fewer boards or start a new canvas.
- Sample content (names, text, covers) is invented and neutral. Do not use real book titles or cover art.
- Colours come from tokens. Placeholder art colours are the only exception.
- A canvas the user has edited is read again before it is changed.

## 7. Check script

Copy the project files to a folder, save the script below as `check-handoff.py`, and run `python3 check-handoff.py <folder>`. Errors must be zero before a handoff is delivered. It checks that master files exist, that no file or reference uses an update or per-chat name, that every referenced file exists, that decision numbers are unique, continuous and cited correctly, that design-system sections and version agree, and that every spec has its required parts.

```python
#!/usr/bin/env python3
"""Kalam handoff check. Usage: python3 check-handoff.py <folder with the project files>"""
import os, re, sys
from collections import Counter
D = sys.argv[1] if len(sys.argv) > 1 else '.'
files = sorted(os.listdir(D))
md = [f for f in files if f.endswith('.md')]
rd = lambda f: open(os.path.join(D, f), encoding='utf-8').read()
err, warn = [], []

# 1. master files exist; no update or per-chat files
for f in ['decisions.md', 'design-system.md', 'kalam-screens.md', 'HANDOFF.md']:
    if f not in files: err.append(f'missing master file {f}')
for f in files:
    if re.search(r'-update|-chat\d', f): err.append(f'{f}: merge into the master files and remove')

# 2. every file named in backticks exists; no stale update-file names
for f in md:
    t = rd(f)
    for m in re.finditer(r'`([A-Za-z0-9_.-]+\.md)`', t):
        if m.group(1) not in files: err.append(f'{f}: refers to missing file {m.group(1)}')
    for m in re.finditer(r'[A-Za-z0-9_-]*(?:-update|-chat\d)[A-Za-z0-9_-]*\.md', t):
        err.append(f'{f}: refers to an update file {m.group(0)}')

# 3. decision numbers: unique, contiguous, and every reference exists
d = rd('decisions.md')
nums = [int(m.group(1)) for m in re.finditer(r'(?m)^(\d+)\. ', d)]
top = max(nums)
for n, c in Counter(nums).items():
    if c > 1: err.append(f'decisions.md: number {n} used {c} times')
gaps = sorted(set(range(1, top + 1)) - set(nums))
if gaps: err.append(f'decisions.md: missing numbers {gaps}')
for f in md:
    for m in re.finditer(r'[Dd]ecisions? (\d+)(?:\s*(?:to|and)\s*(\d+))?', rd(f)):
        for g in m.groups():
            if g and int(g) > top: err.append(f'{f}: cites decision {g}, last is {top}')
if not re.search(rf'Numbering continues from {top}\.', d): warn.append(f'decisions.md: footer should say "Numbering continues from {top}."')

# 4. design-system sections and version
s = rd('design-system.md')
secs = [int(x) for x in re.findall(r'(?m)^## (\d+)\. ', s)]
if secs != list(range(1, len(secs) + 1)): err.append(f'design-system.md: section numbers not 1..N in order: {secs}')
vers = re.findall(r'Additions v(\d+\.\d+)', s)
title = re.search(r'\(v(\d+\.\d+)\)', s.split('\n')[0])
if not title or (vers and title.group(1) != vers[-1]): err.append('design-system.md: title version differs from the latest Additions block')
for f in md:
    for m in re.finditer(r'[Ss]ections? (\d+)', rd(f)):
        if int(m.group(1)) > len(secs) and f != 'HANDOFF.md': warn.append(f'{f}: cites section {m.group(1)}, last is {len(secs)}')

# 5. specs have the required parts
for f in md:
    if f.endswith('-spec.md'):
        t = rd(f)
        if 'Source boards:' not in t or 'https://claude.ai/artifact/' not in t: err.append(f'{f}: no "Source boards:" line with a canvas link')
        if not re.search(r'(?m)^## \d+\. States', t): err.append(f'{f}: no States section')
        if not re.search(r'(?m)^## \d+\. GTK', t): err.append(f'{f}: no GTK widgets section')
        if 'Tokens' not in t: warn.append(f'{f}: no tokens line or section')

# 6. checklist
k = rd('kalam-screens.md')
print('checklist: %d done, %d partly, %d to do' % (k.count('- [x]'), k.count('- [~]'), k.count('- [ ]')))
print(f'decisions: 1 to {top}; design-system: sections 1 to {len(secs)}, v{title.group(1) if title else "?"}')
for w in warn: print('WARN ', w)
for e in err: print('ERROR', e)
print('OK' if not err else f'{len(err)} error(s)')
sys.exit(1 if err else 0)
```
