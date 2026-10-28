#!/bin/sh
echo "Running Ingestooor With Local Validator"

echo "Failure on next line is fine, it's a one time success"
docker network create shared-backend

if [ -z "$1" ]
then
    echo "Reusing existing volumes and containers"
    docker-compose -f docker-compose.local-validator.yml up
else
    echo "Recreating volumes and containers"
    docker-compose -f docker-compose.local-validator.yml up --build --force-recreate --renew-anon-volumes
fi
