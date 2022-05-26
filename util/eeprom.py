#!/usr/bin/python3

""" eeprom.py
Maybe keeping notes about the EEPROM format here.
"""

from sys import argv
from struct import iter_unpack
from hexdump import hexdump

if len(argv) < 2: 
    print("usage: {} <EEPROM dump>".format(argv[0]))
    exit()

with open(argv[1], "rb") as f:
    data = f.read()

print(hexdump(data))
