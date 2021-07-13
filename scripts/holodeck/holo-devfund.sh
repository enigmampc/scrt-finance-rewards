#!/bin/bash

set -e

function wait_for_tx() {
  until (secretcli q tx "$1"); do
      sleep 5
  done
}

export wasm_path=build
export revision="2"

export deployer_name=holotest
export deployer_address=$(secretcli keys show -a $deployer_name)
echo "Deployer address: '$deployer_address'"
export viewing_key="123"
echo "Viewing key: '$viewing_key'"

export gov_addr="secret12q2c5s5we5zn9pq43l0rlsygtql6646my0sqfm"
export token_code_hash="c7fe67b243dfedc625a28ada303434d6f5a46a3086e7d2b5063a814e9f9a379d"
export master_addr="secret13hqxweum28nj0c53nnvrpd23ygguhteqggf852"
export master_code_hash="c8555c2de49967ca484ba21cf563c2b27227a39ad6f32ff3de9758f20159d2d2"

echo "Storing Dev Fund"
resp=$(secretcli tx compute store "${wasm_path}/dev_fund.wasm" --from "$deployer_name" --gas 3000000 -b block -y)
echo $resp
devfund_code_id=$(echo $resp | jq -r '.logs[0].events[0].attributes[] | select(.key == "code_id") | .value')
echo "Stored devfund: '$devfund_code_id'"

echo "Deploying Dev Fund Token.."
export TX_HASH=$(
  secretcli tx compute instantiate $devfund_code_id '{"master":{"address":"'"$master_addr"'", "contract_hash":"'"$master_code_hash"'"},"sefi":{"address":"'"$gov_addr"'", "contract_hash":"'"$token_code_hash"'"}, "viewing_key":"'"$viewing_key"'"}' --from $deployer_name --gas 1500000 --label sefi-dev-fund-$revision -b block -y |
  jq -r .txhash
)
wait_for_tx "$TX_HASH" "Waiting for tx to finish on-chain..."
secretcli q compute tx $TX_HASH
devfund_addr=$(secretcli query compute list-contract-by-code $devfund_code_id | jq -r '.[-1].address')
echo "Dev Fund address: '$devfund_addr'"

devfund_hash="$(secretcli q compute contract-hash "$devfund_addr")"
devfund_hash="${devfund_hash:2}"

echo "Set weight"
export TX_HASH=$(
  secretcli tx compute execute "$master_addr" '{"set_weights":{"weights":[{"address":"'"$devfund_addr"'","hash":"'"$devfund_hash"'","weight":33}]}}' --from $deployer_name --gas 500000 -b block -y |
  jq -r .txhash
)
wait_for_tx "$TX_HASH" "Waiting for tx to finish on-chain..."
secretcli q compute tx $TX_HASH

echo "Dev Fund address: '$devfund_addr'"