# before execution: create baryon_testwallet in keyring-backend test in NEUTRON_DIR/data/baryon-1 directory
# test in baryon-1 testnet
CONTRACT=../artifacts/credits.wasm
CHAINID=baryon-1
NEUTRON_DIR=${NEUTRON_DIR:-../neutron}
HOME=${NEUTRON_DIR}/data/baryon-1/
NEUTROND_BIN=neutrond
NODE=https://rpc.baryon.ntrn.info
TEST_WALLET=baryon_testwallet
TEST_ADDR=$(${NEUTROND_BIN} keys show ${TEST_WALLET} --keyring-backend test -a --home ${HOME})

echo "Store contract"
RES=$(${NEUTROND_BIN} tx wasm store ${CONTRACT} \
    --from ${TEST_ADDR} \
    --gas 50000000 \
    --chain-id ${CHAINID} \
    --broadcast-mode=block \
    --gas-prices 0.0025stake -y \
    --output json \
    --keyring-backend test \
    --home ${HOME} \
    --node ${NODE})
CREDITS_CONTRACT_CODE_ID=$(echo $RES | jq -r '.logs[0].events[1].attributes[0].value')
echo $RES
echo $CREDITS_CONTRACT_CODE_ID

INIT_CREDITS_CONTRACT_MSG='{"when_claimable": "1676016745597000", "dao_address": "${TEST_ADDR}", "airdrop_address": "${TEST_ADDR}", "sale_contract_address": "${TEST_ADDR}", "lockdrop_address": "${TEST_ADDR}"}'

echo "Instantiate"
RES=$(${NEUTROND_BIN} tx wasm instantiate $CREDITS_CONTRACT_CODE_ID \
    "$INIT_CREDITS_CONTRACT_MSG" \
    --from ${TEST_ADDR} \
    --admin ${TEST_ADDR} -y \
    --chain-id ${CHAINID} \
    --output json \
    --broadcast-mode=block \
    --label "init" \
    --keyring-backend test \
    --gas-prices 0.0025untrn \
    --home ${HOME} \
    --node ${NODE})
echo $RES
CREDITS_CONTRACT_ADDRESS=$(echo $RES | jq -r '.logs[0].events[0].attributes[0].value')
echo $CREDITS_CONTRACT_ADDRESS

echo "Mint"
RES=$(${NEUTROND_BIN} tx wasm execute $CREDITS_CONTRACT_ADDRESS \
    '{"mint":{}}' \
    --amount "500untrn" \
    --from ${TEST_ADDR}  -y \
    --chain-id ${CHAINID} \
    --output json \
    --broadcast-mode=block \
    --gas-prices 0.0025untrn \
    --gas 1000000 \
    --keyring-backend test \
    --home ${HOME} \
    --node ${NODE})
echo $RES | jq

echo "Query config"
QUERY_MSG="{\"config\":{}}"
RES=$(${BIN} query wasm contract-state smart ${CREDITS_CONTRACT_ADDRESS} \
    "${QUERY_MSG}" \
    --chain-id "$NEUTRON_CHAIN_ID" \
    --output json \
    --node ${NODE})
echo "$RES" | jq
