@echo off
docker build -t spoticord:latest .
docker image tag spoticord:latest tuxick/spoticord:latest
docker image push tuxick/spoticord:latest