# Example generation images

Each directory is one generation image:

```
<image>/
├── kernel
├── initrd
└── ...        # rootfs or userspace payload in a real build
```

`agentd generation stage <dir>` copies the directory under
`$TIAKI_HOME/generations/<id>/image` and records its `sha256:` directory
digest. `boot` re-hashes the directory before invoking kexec, so a tampered
image never runs.

`base/` and `candidate/` are placeholders with different kernel text so the
demo produces distinct digests. The real image build pipeline replaces them.
