# Keep insertion separate from execution

A Dirgo suggestion can replace or append text in the editable buffer but can
never submit it. This adds one explicit user action compared with automatic
execution, while preventing a ranking, history, parsing, or integration defect
from running an unintended command.
