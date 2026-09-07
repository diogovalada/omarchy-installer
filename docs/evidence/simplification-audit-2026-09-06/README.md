# Setup flow simplification review

Scope: the current browser preview's overview and download screens. Screenshots
were captured and inspected during this review on 2026-09-06. No implementation
changes were made in this review.

1. Overview (`01-overview.jpg`): recognizable Omarchy identity, but the sidebar
   duplicates all three action cards. Multiple slogans compete with the actual
   setup task. The image and introduction push the actions down the screen.
   Recommendation: one setup screen with compact branding, a download panel,
   and the two subsequent actions.
2. Download (`02-download.jpg`): verification is explained clearly but repeated
   across title, paragraph, panel labels, and a separate verification section.
   Recommendation: filename/version, destination, Download button, progress,
   and a concise Verified state. Put detailed checks behind a Details control.

Proposed sequence: show Downloads as the default destination with Change;
download on explicit button click; show download progress followed by Verifying;
after all existing integrity/authenticity checks succeed, show Verified and
enable supported Create USB / Install actions. A downloaded ISO alone cannot
enable an unsupported backend. The current direct-install action stays disabled
with its actual reason until implemented. An existing verified image should
restore readiness rather than force another download.

Accessibility considerations: pair the success checkmark with the word Verified;
do not communicate readiness through green or gray alone. Explain disabled
actions with one shared short sentence. Keyboard access, focus management and
screen-reader announcements need checking when the proposed flow is implemented.

Limits: browser preview blocks native downloads. This review does not establish
native progress behavior, filesystem destination handling, USB operations, or
complete accessibility compliance.
