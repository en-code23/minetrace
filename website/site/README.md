# MineTrace download site

This directory is the static GitHub Pages site for MineTrace. It has no build step: `index.html`, `styles.css`, local images, and the downloadable installers are published as-is.

The repository workflow at `.github/workflows/pages.yml` deploys this directory whenever `main` changes.

For a new release:

1. Replace the files in `downloads/`.
2. Update their filenames, version labels, and sizes in `index.html`.
3. Update `downloads/SHA256SUMS.txt`.
4. Commit and push to `main`.
