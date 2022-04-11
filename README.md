
# Witness

Command line utility which lets you execute arbitrary commands in response to:

- File changes
- UDP packets and TCP connections


## Motivation

While writing code it is often necessary to run you compiler/build tool as you
are editing a file in order to catch errors. Switching back and forth between
your editor and terminal can quickly become tedious and time-consuming.

`witness` helps speed up this workflow: all you have to do is save the file you
are currently working on, and watch the command run.


## Usage

Wait for a file to be modified and run you build tool of choice:

```sh
$ witness cargo build
```

Only watch for files with these specific extensions

```sh
$ witness -e rs cargo build
```

Watch files within a specific directory

```sh
$ witness --path src cargo build
```

Note that anything put within quotes (`"..."`) will be passed to your default
shell, meaning everything you are familiar with from your terminal will work
here as well! This includes pipes, which can be useful if you want to see the
top errors first:

```sh
witness "cargo check |& less"
```


### Other Triggers

`witness` was built around the idea that you might have more complex workflows
than just edit-compile-debug. In addition to file system changes, `witness` can
also be configured to look for IP requests on UDP and TCP sockets. This can be
helpful if you need commands running in one terminal to trigger commands in
other terminals.

Below we will look into a typical workflow involving the use of a web server.
For this use case we keep one terminal open for a edit-compile-debug workflow,
and another terminal for actually running our server in the background.

In our terminal running the server we run the following:

```sh
$ witness --udp=1234 cargo run
```

This tells `witness` to rebuild and rerun your server every time there's a
UDP packet on port 1234.

In our other terminal we then run:

```sh
$ witness "cargo check && witness --trigger --udp=1234"
```

Which will run `cargo check` repeatedly as we make changes to our code. If our
code successfully compiled, `witness` then triggers the other terminal by
sending a UDP packet to port 1234.
