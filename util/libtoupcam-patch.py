#!/usr/bin/python3
""" libtoupcam-patch.py
Tiny patch to enable debug logging in libtoupcam.so
"""

import hashlib
from sys import argv

# The offsets are probably different for other versions of the library
SHA1_DIGEST = "fc795a438afa4c294c35f02077492fa463dfd60d"

if len(argv) < 2:
    print("usage: {} <path to libtoupcam.so>".format(argv[0]))
    exit()

with open(argv[1], "rb") as f:
    data = bytearray(f.read())

d = hashlib.sha1()
d.update(data)
hd = d.hexdigest()

if hd != SHA1_DIGEST:
    print("Unsupported version of libtoupcam.so")
    print("Incorrect SHA1 digest {}, should be {}".format(hd, SHA1_DIGEST))
    exit()

# Magic flags for writing debug output to ./toupcam.log
data[0x00817f79] = 0x82

with open("./libtoupcam_dbg.so", "wb") as f:
    f.write(data)

print("Wrote patched library to ./libtoupcam_dbg.so")
print("You can set $LD_PRELOAD to use this, i.e.")
print("")
print("  $ gcc test.c -o test -ltoupcam")
print("  $ touch toupcam.log")
print("  $ LD_PRELOAD=./libtoupcam_dbg.so ./test")
print("")


