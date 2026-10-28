#!/bin/sh

Help()
{
   # Display Help
   echo "Syntax: ./build.sh [-n|-h]"
   echo "options:"
   echo "  -h     Print this Help."
   echo "  -n     Do not prune intermediate images."
   echo
}

#defaults
prune=0
while getopts "nh" flag
do
    case "${flag}" in
        n) prune=0;;
        h) Help
          exit;;
        \?) Help
          exit;;
    esac
done

echo "Building Base Image"
docker build -t step-ingestooor . || { echo "Build services docker failed"; exit 3; }

if [ $prune -eq 1 ] 
then
  echo "Pruning"
  docker system prune -f || (echo "Prune failed" && exit 4)
fi