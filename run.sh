#!/bin/sh
echo "Running Ingestooor"

echo "Failure on next line is fine, it's a one time success"
docker network create shared-backend

if [ -z "$1" ]
then
    echo "Reusing existing volumes and containers"
    docker-compose -f docker-compose.dev.yml up
else
    echo "Recreating volumes and containers"
    docker-compose -f docker-compose.dev.yml up --build --force-recreate --renew-anon-volumes
fi
