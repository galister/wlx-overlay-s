#!/bin/bash
set -e
cd "$(dirname "$0")"

bun install

export MODEL="gemma3:27b"

TEMPLATE="pl" bun main.ts
TEMPLATE="de" bun main.ts
TEMPLATE="ja" bun main.ts
TEMPLATE="es" bun main.ts
TEMPLATE="it" bun main.ts
TEMPLATE="zh_CN" bun main.ts
