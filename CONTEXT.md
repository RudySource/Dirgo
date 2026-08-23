# Dirgo Navigation

Dirgo is a terminal navigation layer that resolves human intent to a directory without expanding into general file management.

## Language

**Indexed directory**:
A directory known through Dirgo's disposable filesystem snapshot, whether or not the user has visited it.
_Avoid_: Cached path, visited directory

**Navigation**:
A successful request for the caller shell to change its working directory to a resolved directory.
_Avoid_: Jump, open

**Resolution**:
The decision that a query identifies one directory with enough confidence to navigate without asking the user.
_Avoid_: Search, match

**Candidate**:
An indexed directory that matches a query but has not necessarily met the confidence threshold for resolution.
_Avoid_: Result, destination

**Picker**:
The interactive surface used when no query was supplied, selection was forced, or candidates are ambiguous.
_Avoid_: File manager, browser

**Bookmark**:
A user-owned, persistent name for a directory that survives rebuilding or deleting the filesystem index.
_Avoid_: Alias, favorite

**Visit**:
A navigation completed through Dirgo and recorded for ranking.
_Avoid_: Shell history entry

**Navigation session**:
The ordered, branch-aware sequence of Dirgo navigations owned by one shell process.
_Avoid_: Global history

**Project root**:
A directory identified as the boundary of a source project by one or more configured marker files.
_Avoid_: Repository, workspace

**Filesystem index**:
A disposable, rebuildable snapshot of directories under configured roots.
_Avoid_: History database, state

**User state**:
Persistent bookmarks, visits, and navigation sessions that must never be discarded as part of index maintenance.
_Avoid_: Cache, index
