#!/bin/bash

# The URL to download
url="https://api.mainnet-beta.solana.com/snapshot.tar.bz2"

# Get the final URL after redirects without downloading the file
echo "Resolving final URL..."
final_url=$(wget --server-response --spider -O- "$url" 2>&1 | grep -i 'Location:' | tail -n 1 | awk '{print $2}' | tr -d '\r')

echo "Resolved URL: $final_url"

# Extract the filename from the final URL
filename=$(basename "$final_url")

echo "Found file name: $filename. Sleeping for 60 seconds..."

# Sleep to avoid rate limiting. 429s
sleep 60

echo "Downloading $filename..."

# Download the file
wget -O "$filename" "$url"