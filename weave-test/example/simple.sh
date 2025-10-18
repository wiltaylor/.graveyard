#!/bin/sh

assert_pass() {
  echo "WEAVE-TEST:PASS: $1"
}

assert_fail() {
  echo "WEAVE-TEST:FAIL: $1"
}

test_print() {
  echo "WEAVE-TEST:PRINT: $1"
}

case $1 in
  "a")
    test_print "Example of a custom message"
    assert_pass "Test all good"
  ;;

  "b")
    assert_pass "Test b all good"
  ;;

  "failing")
    assert_fail "Oh no something went wrong"
  ;;

  "hangs")
    sleep 100
  ;;

  "dataset")
  test_print "VARA: $VARA VARB: $VARB"
  assert_pass "This row passed"
  ;;

  *)
    # here we don't report anything so we get an inconclusive test
  ;;
esac

