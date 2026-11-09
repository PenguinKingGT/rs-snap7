# TIA Portal Setup

Classic S7 access on S7-1200/1500 requires suitable CPU permissions and a DB with
absolute offsets. Follow the [upstream setup guide](https://python-snap7.readthedocs.io/en/latest/tia-portal-config.html)
and the manual for your CPU/firmware; menu names vary by TIA Portal version.

## Prepare a dedicated DB

1. Select a DB intended for application access.
2. In its attributes, configure standard access so fields have fixed byte offsets.
3. Record the DB number, field types, and offsets for your Rust application.

Changing block access can reinitialize DB values. Plan the change and preserve
required data first. Siemens documents the conversion in
[Setting up block access](https://docs.tia.siemens.cloud/r/en-us/v20/programming-basics/blocks-in-the-user-program/blocks-with-optimized-access/setting-up-block-access).

## Enable and apply communication settings

In CPU properties, locate the protection/security connection settings and enable
remote PUT/GET where supported. Compile and download the approved CPU and DB
changes. Start with a small read from the known fixture; verify its expected bytes
before adding writes. The [upstream walkthrough](https://python-snap7.readthedocs.io/en/latest/tia-portal-config.html)
describes this sequence and the access implications.

PUT/GET exposes memory access; enable it only within your deployment's access
policy. Symbolic access and S7CommPlus are outside this crate's scope.
