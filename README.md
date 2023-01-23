# hashsummer

Utility for calculating checksums of large file bases and finding differences.

**Work in progress... this software not really suitable for production**

HashSummer is a library and command line utility helping with calculating hash checksums of directories with large file
count and summarising everything into small fingerprint file. Such fingerprints can be compared to each other and show
what files were added/removed/changed.

### Example usages
 - Detecting disk data integrity errors where file system doesn't support checksums.
 - Generating list of what changed since last backup, and using other programs to copy/archive only listed files.
 - Other stuff...