#!/bin/bash

set -uo pipefail

STATIC_DOMAINS_FILE="$HOME/.ssh/static_canonical_domains.conf"
CANONICAL_CONFIG="$HOME/.ssh/tmp/canonical_domains.conf"

ACTION="$1"
CONNECTION_NAME="${2:-$CONNECTION_NAME}"
DOMAINS="${3:-$CONNECTION_CONTEXT}"

# Add or remove DOMAINS from CANONICAL_CONFIG depending on $ACTION (up/down)
# Domains in $STATIC_DOMAINS_FILE will always be included
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
    cat "$CANONICAL_CONFIG" | tr " " "\n" | sed '1d'
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
        printf "%s" "$DOMAINS" | tr " " "\n"
    elif [ "$1" = "down" ]; then
        cat | grep -vf <(printf "%s" "$DOMAINS" | tr " " "\n")
    fi
}

# Generate file
( ( (printf "%s\n" "CanonicalDomains"; current_domains; static_domains) \
    | context_domains "$ACTION" \
    | sort -u | tr "\n" " "); printf "\n") \
 | tee "$CANONICAL_CONFIG"

# Delete gen file if there is no entry
grep -E "^CanonicalDomains $" "$CANONICAL_CONFIG" >/dev/null && rm "$CANONICAL_CONFIG"
