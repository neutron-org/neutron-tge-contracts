### USDC converter contract

This contract is designed to facilitate Lockdrop participants migration to Noble USDC. It's only `ConvertAndStake` `Receive` handler processes the following operations:
1. Making a `WithdrawLiquidity` call to the `NTRN<>USDC.axl` pair using a given amount of LP tokens;
2. Swapping the withdrawn USDC.axl to Noble USDC;
3. Making a `ProvideLiquidity` call to the `NTRN<>USDC` pair using the withdrawn amount of NTRN and swapped amount of USDC.

### Call example

Here's a `ConvertAndStake` message run on a mainnet fork explanation:
```json
{
  "messages": [
    {
      "@type": "/cosmwasm.wasm.v1.MsgExecuteContract",
      "sender": "neutron1kyn3jx88wvnm3mhnwpuue29alhsatwzrpkwhu6",
      "contract": "neutron137rzkacryzstfyylvvvu2vq9uj5l89yzx9v7gxp0s77xq5ach2xs3t6t3p",
      "msg": {
        "send": {
          "contract": "neutron1zqskhhcn3t45q6y6ljly6nnwreyjw92ejpewc6aj4cfsuyj8mpxsfnxp56",
          "amount": "10000000",
          "msg": "eyJjb252ZXJ0X2FuZF9zdGFrZSI6eyJ0cmFuc211dGVyX3Bvb2wiOiJuZXV0cm9uMW5zMnRjdW5ybHJrNXlrNjJmcGw3NHljYXphbmNleWZtbXE3ZGxqNnNxOG4wcm5rdXZrN3N6c3RreXgiLCJub2JsZV9wb29sIjoibmV1dHJvbjFxM3JmbWZsaHA3OXJ6MHNkanhkZ2Y3dDN4NXR3Y2U5YXo2bjBmZWdzcGtqc3RyYXg0MmtxMGc5NnJuIiwibm9ibGVfdXNkY19kZW5vbSI6ImliYy9CNTU5QTgwRDYyMjQ5QzhBQTA3QTM4MEUyQTJCRUE2RTVDQTlBNkYwNzlDOTEyQzNBOUU5QjQ5NDEwNUU0RjgxIiwicHJvdmlkZV9saXF1aWRpdHlfc2xpcHBhZ2VfdG9sZXJhbmNlIjoiMC4wMSJ9fQ=="
        }
      },
      "funds": []
    }
  ]
}
```

where the decoded message is
```json
{
  "convert_and_stake": {
    "transmuter_pool": "neutron1ns2tcunrlrk5yk62fpl74ycazanceyfmmq7dlj6sq8n0rnkuvk7szstkyx",
    "noble_pool": "neutron1q3rfmflhp79rz0sdjxdgf7t3x5twce9az6n0fegspkjstrax42kq0g96rn",
    "noble_usdc_denom": "ibc/B559A80D62249C8AA07A380E2A2BEA6E5CA9A6F079C912C3A9E9B494105E4F81",
    "provide_liquidity_slippage_tolerance": "0.01"
  }
}
```
