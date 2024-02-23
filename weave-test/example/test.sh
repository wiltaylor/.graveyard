#!/bin/bash

case $1 in

  "pass")
    echo "WEAVE-TEST:PASS: yay"
    echo "WEAVE-TEST:PRINT: Hello from a test"
    sleep 20
    echo "WEAVE-TEST:PRINT: Finish sleep"
    ;;

  "fail")
    echo "WEAVE-TEST:FAIL: booo"
    ;;

  "inconclusive")
    echo "Nothing"
    ;;

  "skipped")
    echo "WEAVE-TEST:FAIL"
    ;;

  *)
    echo "Doesn't return any status that will be read"
    ;;
esac