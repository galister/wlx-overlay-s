#!/bin/bash
set -e
cd "$(dirname "$0")"

bun install

export MODEL="gemma3:12b"

TEMPLATE="pl" bun main.ts
TEMPLATE="de" bun main.ts
TEMPLATE="ja" bun main.ts
TEMPLATE="es" bun main.ts
