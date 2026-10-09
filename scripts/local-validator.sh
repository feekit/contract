#!/usr/bin/env bash
# Local validator with FeeKit and the pump.fun programs cloned from mainnet.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
ledger="${FEEKIT_LEDGER:-/tmp/feekit-validator}"
url="${FEEKIT_CLONE_URL:-https://api.mainnet-beta.solana.com}"

exec solana-test-validator \
  --reset \
  --ledger "$ledger" \
  --url "$url" \
  --clone-upgradeable-program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P \
  --clone-upgradeable-program pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA \
  --clone-upgradeable-program pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ \
  --clone-upgradeable-program MAyhSmzXzV1pTf7LsNkrNwkWKTo4ougAJ1PPg47MD4e \
  --clone-upgradeable-program TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb \
  --clone So11111111111111111111111111111111111111112 \
  --clone 4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf \
  --clone 8Wf5TiAheLUqBrKXeYg2JtAFFMWtKdG2BSFgqUcPVwTt \
  --clone 5PHirr8joyTMp9JMm6nW7hNDVyEYdkzDqazxPD7RaTjx \
  --clone ADyA8hdefvWN2dbGGWFotbzWxrAvLW83WG6QCVXvJKqw \
  --clone Hq2wp8uJ9jCPsYgNHex8RtqdvMPfVGoYwjvF1ATiwn2Y \
  --clone C2aFPdENg4A2HQsmrd5rTw5TaYBX5Ku887cWjbFKtZpw \
  --clone 13ec7XdrjF3h3YcqBTFDSReRcUFwbCnJaAQspM4j6DDJ \
  --clone BwWK17cbHxwWBKZkUYvzxLcNQ1YVyaFezduWbtm2de6s \
  --maybe-clone Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1 \
  --maybe-clone D6QxXDt6hhcCpto4HiZKkN2YQ2iZRF5R7S3caCHpUsML \
  --maybe-clone GS4CU59F31iL7aR2Q8zVS8DRrcRnXX1yjQ66TqNVQnaR \
  --maybe-clone 62qc2CNXwrYqQScmEdiZFFAnJR262PxWEuNQtxfafNgV \
  --maybe-clone 5YxQFdt3Tr9zJLvkFccqXVUwhdTWJQc1fFg2YPbxvxeD \
  --maybe-clone 94qWNrtmfn42h3ZjUZwWvK1MEo9uVmmrBPd2hpNjYDjb \
  --maybe-clone HjQjngTDqoHE6aaGhUqfz9aQ7WZcBRjy5xB8PScLSr8i \
  --bpf-program 9EMWVqoVNW9armPPwiY7DtW7kgQ14LTgk8F3mCkMZ11C "$root/target/deploy/feekit.so"
