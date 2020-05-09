#!/bin/bash

set -uo pipefail

STATIC_DOMAINS_FILE="$HOME/.ssh/static_canonical_domains.conf"
OUTPUT_DOMAINS_FILE="$HOME/.ssh/tmp/canonical_domains.conf"

ACTION="$1"
CONNECTION_NAME="${2:-$CONNECTION_NAME}"
CONNECTION_CONTEXT="${3:-$CONNECTION_CONTEXT}"

# Will add or remove $CONNECTION_CONTEXT domains from $OUTPUT_DOMAINS_FILE depending of $ACTION
# Domains in $STATIC_DOMAINS_FILE will still be present
# Example of `.ssh/config`:
# ```
# # Don't canonicalize host with dots (assume there are already full hostname)
# CanonicalizeMaxDots 0
#
# # Fallback to local name Resolution in any case
# CanonicalizeFallbackLocal yes
# CanonicalizeHostname yes
#
# # Include static list
# Include static_canonical_domains.conf
#
# # Include script generated list of CanonicalDomains
# Include tmp/canonical_domains.conf
# ```

current_domains() {
    cat "$OUTPUT_DOMAINS_FILE" | tr " " "\n" | sed '1d'
}

static_domains() {
    # Add static canonical domains if they exist
    if [ -f "$STATIC_DOMAINS_FILE" ]; then
        grep -E "^CanonicalDomains " "$STATIC_DOMAINS_FILE" | tr " " "\n" | sed '1d'
    fi
}

context_domains() {
    if [ "$1" = "up" ]; then
        cat
        printf "%s" "$CONNECTION_CONTEXT" | tr " " "\n"
    elif [ "$1" = "down" ]; then
        cat | grep -vf <(printf "%s" "$CONNECTION_CONTEXT" | tr " " "\n")
    fi
}

( ( (printf "%s\n" "CanonicalDomains"; current_domains; static_domains) \
    | context_domains "$ACTION" \
    | sort -u | tr "\n" " "); printf "\n") \
 | tee "$OUTPUT_DOMAINS_FILE"

# Delete gen file if there is no entry
grep -E "^CanonicalDomains $" "$OUTPUT_DOMAINS_FILE" >/dev/null && rm "$OUTPUT_DOMAINS_FILE"
