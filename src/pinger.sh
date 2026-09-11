#!/bin/bash

# add your ping targets here
targets=('8.8.8.8' '8.8.4.4' '1.1.1.1')

if [ $# -ge 2 ]; then
  group=$1
  intf=$2
else
  echo "Testing default interface"
  test_default=true
fi

for host in "${targets[@]}"; do
  if [ $test_default ]; then
    /bin/ping -n -c 1 -W 1 -w1 $host
  else
    echo "Do ping $intf $host"
    /bin/ping -n -c 1 -W 1 -w1 -I $intf $host
  fi
  if [ $? == 0 ]; then
    exit 0
  fi
done

# fail
exit 1
