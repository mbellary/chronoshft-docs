# Chronoshft HFT documentation

An evolving HFT learning path built with [MkDocs](https://www.mkdocs.org/).

## Preview locally

Use Python 3.10 or newer:

```sh
python3 -m venv .venv
source .venv/bin/activate
python -m pip install -r requirements.txt
mkdocs serve
```

Open <http://127.0.0.1:8000/chronoshft-docs/>. Changes to Markdown and configuration reload automatically.

## Build and validate

```sh
mkdocs build --strict
```

The generated site is written to `site/`, which is excluded from Git. CI runs the same strict build on pushes and pull requests.

## GitHub Pages

The site is configured for <https://mbellary.github.io/chronoshft-docs/>.

For the first deployment, open the repository's **Settings → Pages** and select **GitHub Actions** under **Build and deployment → Source**.

Push changes to `main` to build and deploy automatically. Pull requests run the strict build for validation. You can also run the **Documentation** workflow manually from the Actions tab on `main`.

The workflow uploads `site/` as a Pages artifact and deploys it using GitHub's Pages actions. The deployment job uses the `github-pages` environment and the required Pages and OpenID Connect permissions. A separate `gh-pages` branch is unnecessary.

## Organization

All site content lives in `docs/`, organized into six categories:

| Category | Existing chapters |
| --- | --- |
| Microstructure | Ready for new chapters |
| Systems | Memory alignment and padding |
| Hardware | Memory hierarchy; branch prediction |
| Latency | Prefetching; pipeline flushes |
| Networking | Ready for new chapters |
| Backtesting | Ready for new chapters |

Existing diagrams, standalone HTML editions, and verification notes are kept alongside their chapters. Companion Rust sources were copied from the original local chapter outputs, excluding compiled artifacts. Historical verification notes describe the original lab runs; the MkDocs build validates the documentation site.

## Add documentation

1. Add a Markdown file under `docs/<category>/`, using a topic subdirectory for chapters with diagrams or companion sources.
2. Use a single `#` heading for the page title and relative links for pages and assets. Link to Markdown source paths; MkDocs rewrites page links during the build.
3. Add the page to the corresponding category in `mkdocs.yml` and link it from that category's `index.md`.
4. Run `mkdocs build --strict` before committing.

Use category overviews to cross-link related topics without duplicating chapters. Keep lab build output in `target/` directories, which are excluded from Git.
