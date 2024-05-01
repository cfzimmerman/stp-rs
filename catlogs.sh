#!/bin/bash

# Shortcut for printing all the log files

cd logs

for FILE in *; do
	echo -e "\n$FILE"
	cat $FILE
done
