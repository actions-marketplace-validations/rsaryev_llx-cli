The release is three commands, in this order.

> [!STEPS] Cut a release
> 1. **Run the checks**
>    Everything CI runs, in CI's order.
>
>    ```sh
>    scripts/check --all
>    ```
> 2. **Tag it**
>    The tag names the version; the workflow does the rest.
>
>    ```sh
>    git tag v0.6.0 && git push --tags
>    ```
> 3. **Watch the archives land**
>    macOS and Linux, each with its `SHA256SUMS`.

> [!WARNING] The size gate
> A binary above 5 MiB fails the release.
