#!/bin/bash

set -e

function wait_for_tx() {
  until (secretcli q tx "$1"); do
      sleep 5
  done
}

export wasm_path=build
export revision="12"

export deployer_name=holotest
export deployer_address=$(secretcli keys show -a $deployer_name)
echo "Deployer address: '$deployer_address'"
export viewing_key="123"
echo "Viewing key: '$viewing_key'"

export sscrt_addr="secret1s7c6xp9wltthk5r6mmavql4xld5me3g37guhsx"
export gov_addr="secret12q2c5s5we5zn9pq43l0rlsygtql6646my0sqfm"
export token_code_hash="c7fe67b243dfedc625a28ada303434d6f5a46a3086e7d2b5063a814e9f9a379d"
export master_addr="secret13hqxweum28nj0c53nnvrpd23ygguhteqggf852"
export master_code_hash="c8555c2de49967ca484ba21cf563c2b27227a39ad6f32ff3de9758f20159d2d2"
export pair_hash="f86b5c3ca0381ce7edfffa534789501ae17cf6b21515213693baf980765729c2"
export pair1="secret16krcdrqh6y6pazvkj58nrvkerk0q0ttg22kepl" # sSCRT/SCRT
export pair2="secret1l56ke78aj9jxr4wu64h4rm20cnqxevzpf6tmfc" # sSCRT/SEFI

echo "Storing CSHBK"
resp=$(secretcli tx compute store "${wasm_path}/cashback_token.wasm" --from "$deployer_name" --gas 3000000 -b block -y)
echo $resp
cashback_code_id=$(echo $resp | jq -r '.logs[0].events[0].attributes[] | select(.key == "code_id") | .value')
echo "Stored cashback: '$cashback_code_id'"

echo "Deploying Cashback Token.."
export TX_HASH=$(
  secretcli tx compute instantiate $cashback_code_id '{"prng_seed":"YWE=", "master":{"address":"'"$master_addr"'", "hash":"'"$master_code_hash"'"},"sefi":{"address":"'"$gov_addr"'", "hash":"'"$token_code_hash"'"}}' --from $deployer_name --gas 1500000 --label CSHBK-$revision -b block -y |
  jq -r .txhash
)
wait_for_tx "$TX_HASH" "Waiting for tx to finish on-chain..."
secretcli q compute tx $TX_HASH
cashback_addr=$(secretcli query compute list-contract-by-code $cashback_code_id | jq -r '.[-1].address')
echo "CSHBK address: '$cashback_addr'"

cashback_hash="$(secretcli q compute contract-hash "$cashback_addr")"
cashback_hash="${cashback_hash:2}"

echo "Set weight"
export TX_HASH=$(
  secretcli tx compute execute "$master_addr" '{"set_weights":{"weights":[{"address":"'"$cashback_addr"'","hash":"'"$cashback_hash"'","weight":33}]}}' --from $deployer_name --gas 500000 -b block -y |
  jq -r .txhash
)
wait_for_tx "$TX_HASH" "Waiting for tx to finish on-chain..."
secretcli q compute tx $TX_HASH

echo "Storing CSHBK Minter"
resp=$(secretcli tx compute store "${wasm_path}/cashback_minter.wasm" --from "$deployer_name" --gas 3000000 -b block -y)
echo $resp
minter_code_id=$(echo $resp | jq -r '.logs[0].events[0].attributes[] | select(.key == "code_id") | .value')
echo "Stored cashback minter: '$minter_code_id'"

echo "Deploying Cashback Minter.."
export TX_HASH=$(
  secretcli tx compute instantiate $minter_code_id '{"sscrt_addr":"'"$sscrt_addr"'", "pairs":["'"$pair1"'","'"$pair2"'"], "pair_contract_hash":"'"$pair_hash"'", "cashback":{"address":"'"$cashback_addr"'","contract_hash":"'"$cashback_hash"'"}}' --from $deployer_name --gas 1500000 --label cb-minter-$revision -b block -y |
  jq -r .txhash
)
wait_for_tx "$TX_HASH" "Waiting for tx to finish on-chain..."
secretcli q compute tx $TX_HASH
minter_addr=$(secretcli query compute list-contract-by-code $minter_code_id | jq -r '.[-1].address')
echo "Minter address: '$minter_addr'"

#echo "Set data sender"
#export TX_HASH=$(
#  secretcli tx compute execute "$cashback_addr" '{"set_data_sender":{"address":"'"$router_addr"'"}}' --from $deployer_name --gas 500000 -b block -y |
#  jq -r .txhash
#)
#wait_for_tx "$TX_HASH" "Waiting for tx to finish on-chain..."
#secretcli q compute tx $TX_HASH

echo "Set minter"
export TX_HASH=$(
  secretcli tx compute execute "$cashback_addr" '{"add_minters":{"minters":["'"$minter_addr"'"]}}' --from $deployer_name --gas 500000 -b block -y |
  jq -r .txhash
)
wait_for_tx "$TX_HASH" "Waiting for tx to finish on-chain..."
secretcli q compute tx $TX_HASH

echo "Cashback address: '$cashback_addr'"
echo "minter address: '$minter_addr'"