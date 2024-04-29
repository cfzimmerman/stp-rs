#!/bin/bash

cd logs

for FILE in *; do
	echo -e "\n$FILE"
	cat $FILE
done
