Install it whichever way the machine already has.

> [!TABS] Install
> #### curl
> ```sh
> curl -fsSL https://llx.sh/install | sh
> ```
> #### Docker
> ```sh
> docker run --rm -it ghcr.io/localixai/localix-cli
> ```
> #### From source
> Needs a stable Rust toolchain.
>
> ```sh
> cargo install --path apps/cli
> ```

> [!TIP]
> `llx doctor` says what is missing once it is installed.
