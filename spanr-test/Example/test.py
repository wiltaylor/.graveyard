#!/usr/bin/python

import sys

if sys.argv[1] == "testpass":
    print("##PASS##")
if sys.argv[1] == "testignore":
    print("##IGNORE##")
if sys.argv[1] == "testwarn":
    print("##WARN##")
if sys.argv[1] == "testfail":
    print("##FAIL##")

sys.exit(0)