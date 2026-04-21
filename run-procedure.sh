#!/bin/sh

KEY_CLIENT_ID="$ZELA_PROJECT_KEY_ID"
KEY_SECRET="$ZELA_PROJECT_KEY_SECRET"

PROCEDURE="${1}"
PARAMS="${2}"

usage() {
	echo "usage: ZELA_PROJECT_KEY_ID=key_id ZELA_PROJECT_KEY_SECRET=key_secret run-procedure.sh procedure#<git-sha> '{ \"json\": \"params\" }'"
	echo "  <git-sha> is the full SHA1 commit hash from the build row in the dashboard."
}

if [ -z "$PROCEDURE" ] || [ -z "$PARAMS" ] || [ -z "$KEY_CLIENT_ID" ] || [ -z "$KEY_SECRET" ]; then
	usage
	exit 1
fi

ZELA_JWT=$(curl -sS --user "$KEY_CLIENT_ID:$KEY_SECRET" \
	--data-urlencode 'grant_type=client_credentials' \
	--data-urlencode 'scope=zela-executor:call' \
	https://auth.zela.io/realms/zela/protocol/openid-connect/token | jq -r .access_token)

# Little bit stupid but we print the output of the request to stderr and capture timing information on stdout
stats=$(curl -s \
	--write-out '%{stdout}%{time_starttransfer} %{time_pretransfer}' \
	--output /dev/stderr \
	--header "Authorization: Bearer $ZELA_JWT" \
	--header 'Content-type: application/json' \
	--data "{ \"jsonrpc\": \"2.0\", \"id\": 1, \"method\": \"zela.$PROCEDURE\", \"params\": $PARAMS }" \
	https://executor.zela.io)

# Then we compute the subtraction of timing
req_time=$(echo "$stats" | awk '{ print $1 - $2 }')
echo "\nRequest time: ${req_time}s"
