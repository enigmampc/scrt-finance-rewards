#!/bin/bash

set -e

function wait_for_tx() {
  until (secretcli q tx "$1"); do
      sleep 5
  done
}

export HOUR=3600
export DAY=$((HOUR * 24))
export WEEK=$((DAY * 7))

export wasm_path=build
export revision="9"

export deployer_name=test
export deployer_address=$(secretcli keys show -a $deployer_name)
echo "Deployer address: '$deployer_address'"
export viewing_key="123"
echo "Viewing key: '$viewing_key'"

export gov_addr="secret12q2c5s5we5zn9pq43l0rlsygtql6646my0sqfm"
export token_code_hash="c7fe67b243dfedc625a28ada303434d6f5a46a3086e7d2b5063a814e9f9a379d"
export master_addr="secret13hqxweum28nj0c53nnvrpd23ygguhteqggf852"
export master_code_hash="c8555c2de49967ca484ba21cf563c2b27227a39ad6f32ff3de9758f20159d2d2"
export sefi_staking_addr="secret1c6qft4w76nreh7whn736k58chu8qy9u57rmp89"
export sefi_staking_hash="8fcc4c975a67178b8b15b903f99604c2a38be118bcb35751ffde9183a2c6a193"
export sefi_staking_vk="api_key_vQGLKACbvr3DEUdffavJFkhbr1LF6lH9yEmhxkeRXQo="

export vote_duration=$((HOUR))
export quorum=33
export min_staked="1000000" # 1 SEFI

#echo "Storing SEFI Staking"
#resp=$(secretcli tx compute store "${wasm_path}/lp_staking.wasm" --from "$deployer_name" --gas 3000000 -b block -y)
#echo $resp
#sefi_staking_code_id=$(echo $resp | jq -r '.logs[0].events[0].attributes[] | select(.key == "code_id") | .value')
#echo "Stored lp staking: '$sefi_staking_code_id'"
#
#echo "Deploying SEFI Staking Contract.."
#export TX_HASH=$(
#  secretcli tx compute instantiate $sefi_staking_code_id '{"reward_token":{"address":"'"$gov_addr"'", "contract_hash":"'"$token_code_hash"'"},"inc_token":{"address":"'"$gov_addr"'", "contract_hash":"'"$token_code_hash"'"},"master":{"address":"'"$master_addr"'", "contract_hash":"'"$master_code_hash"'"},"viewing_key":"'"$viewing_key"'","token_info":{"name":"sefis","symbol":"SEFISTAKING"},"prng_seed":"YWE="}' --from $deployer_name --gas 1500000 --label 'sefi-stake(voting)-'"$revision" -b block -y |
#  jq -r .txhash
#)
#wait_for_tx "$TX_HASH" "Waiting for tx to finish on-chain..."
#secretcli q compute tx $TX_HASH
#sefi_staking_addr=$(secretcli query compute list-contract-by-code $sefi_staking_code_id | jq -r '.[-1].address')
#echo "SEFI Staking address: '$sefi_staking_addr'"
#
#sefi_staking_hash="$(secretcli q compute contract-hash "$sefi_staking_addr")"
#sefi_staking_hash="${sefi_staking_hash:2}"
#
#echo "Setting SEFI Staking weight.."
#export TX_HASH=$(
#  secretcli tx compute execute "$master_addr" '{"set_weights":{"weights":[{"address":"'"$sefi_staking_addr"'","hash":"'"$sefi_staking_hash"'","weight":99}]}}' --from $deployer_name --gas 1500000 -b block -y |
#  jq -r .txhash
#)
#wait_for_tx "$TX_HASH" "Waiting for tx to finish on-chain..."
#secretcli q compute tx $TX_HASH

echo "Storing vote factory"
resp=$(secretcli tx compute store "${wasm_path}/poll_factory.wasm" --from "$deployer_name" --gas 3000000 -b block -y)
echo $resp
factory_code_id=$(echo $resp | jq -r '.logs[0].events[0].attributes[] | select(.key == "code_id") | .value')
echo "Stored voting factory: '$factory_code_id'"

echo "Storing vote contract"
resp=$(secretcli tx compute store "${wasm_path}/secret_poll.wasm" --from "$deployer_name" --gas 3000000 -b block -y)
echo $resp
vote_code_id=$(echo $resp | jq -r '.logs[0].events[0].attributes[] | select(.key == "code_id") | .value')
vote_code_hash=$(secretcli q compute list-code | jq '.[] | select(.id == '"$vote_code_id"') | .data_hash')
echo "Stored voting factory: '$vote_code_id', '$vote_code_hash'"

echo "Deploying Vote Factory.."
export TX_HASH=$(
secretcli tx compute instantiate $factory_code_id '{"prng_seed":"YWE=","poll_contract":{"code_id":'"$vote_code_id"',"code_hash":'"$vote_code_hash"'},"staking_pool":{"address":"'"$sefi_staking_addr"'","contract_hash":"'"$sefi_staking_hash"'"},"default_poll_config":{"duration":'"$vote_duration"', "quorum":'"$quorum"', "min_threshold":0},"min_staked":"'"$min_staked"'", "reveal_com":{"n":1, "revealers":["secret1k89hg6e5fxkeya9x6vq6yxzs76zt7xkj943tav","secret1p0vgghl8rw4ukzm7geyy0f0tl29glxrtnlalue"]}}' --from $deployer_name --gas 1500000 --label vote_factory-$revision -b block -y |
  jq -r .txhash
)
wait_for_tx "$TX_HASH" "Waiting for tx to finish on-chain..."
secretcli q compute tx $TX_HASH
vote_factory_addr=$(secretcli query compute list-contract-by-code $factory_code_id | jq -r '.[-1].address')
vote_factory_code_hash=$(secretcli q compute list-code | jq '.[] | select(.id == '"$factory_code_id"') | .data_hash')
echo "Vote Factory address: '$vote_factory_addr', '$vote_factory_code_hash'"

echo "Adding vote factory as a subscriber.."
export TX_HASH=$(
  secretcli tx compute execute $sefi_staking_addr '{"add_subs":{"contracts":[{"address":"'"$vote_factory_addr"'","contract_hash":'"$vote_factory_code_hash"'}]}}' --from $deployer_name --gas 1500000 -b block -y |
  jq -r .txhash
)
wait_for_tx "$TX_HASH" "Waiting for tx to finish on-chain..."
export resp=$(secretcli q compute tx $TX_HASH)
echo $resp

echo "Deploying a vote.."
export TX_HASH=$(
  secretcli tx compute execute $vote_factory_addr '{"new_poll":{"poll_metadata":{"title":"demo vote", "description":"this is a demo vote woohoooooooo!!!", "vote_type":"SEFI Community Spending", "author_addr":"'"$deployer_address"'", "author_alias":"you know who it is"},"poll_choices":["Yes","No"],"pool_viewing_key":"'"$sefi_staking_vk"'"}}' --from $deployer_name --gas 1500000 -b block -y |
  jq -r .txhash
)
wait_for_tx "$TX_HASH" "Waiting for tx to finish on-chain..."
export resp=$(secretcli q compute tx $TX_HASH)
echo $resp
vote_contract_addr=$(echo $resp | jq -r '.output_log[0].attributes[] | select(.key == "new_poll") | .value')
echo "Vote contract address: '$vote_contract_addr'"

echo ""
echo "SEFI Staking address: '$sefi_staking_addr'"
echo "Voting Factory address: '$vote_factory_addr'"
echo "Vote contract address: '$vote_contract_addr'"
