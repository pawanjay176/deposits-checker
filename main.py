# Usage
# python3 main.py <TO-CHECK-ENDPOINT> <TRUSTED-ENDPOINT>

import requests
import json
import sys

HEADERS = {'Content-type': 'application/json'}
TIMEOUT = 60
DEPOSIT_CONTRACT= "0x00000000219ab540356cBB839Cbe05303d7705Fa" # Mainnet
START_BLOCK = 11184524
DEPOSIT_EVENT_TOPIC = "0x649bbc62d0e31342afea4e5cd82d4049e7e1ee912fc0889aa790803be39038c5"

def send_rpc_request(endpoint, method, params):
    body = {
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": 1
    }
    resp = requests.post(endpoint, data = json.dumps(body), headers = HEADERS, timeout = TIMEOUT)
    return resp.json()

def get_chain_id(endpoint):
    return int(send_rpc_request(endpoint, "eth_chainId", [])['result'], 16)

def get_network_id(endpoint):
    return send_rpc_request(endpoint, "net_version", [])['result']

def get_block_number(endpoint):
    return int(send_rpc_request(endpoint, "eth_blockNumber", [])['result'], 16)

def get_deposit_logs_count(endpoint, start, end):
    params = [{
        "address": DEPOSIT_CONTRACT,
        "topics": [DEPOSIT_EVENT_TOPIC],
        "fromBlock": hex(start),
        "toBlock": hex(end)
    }]
    resp = send_rpc_request(endpoint, "eth_getLogs", params)
    return len(resp['result'])

def main(to_check_endpoint, trusted_endpoint):
    block = get_block_number(trusted_endpoint)
    range_chunks = list(range(START_BLOCK, block, 1000))
    range_chunks = [(range_chunks[i], range_chunks[i+1]) for i in range(len(range_chunks) - 1)]
    good_boy = True
    for r in range_chunks:
        print("Checking in range %d %d" % (r[0], r[1]))
        trusted = get_deposit_logs_count(trusted_endpoint, r[0], r[1])
        to_check = get_deposit_logs_count(to_check_endpoint, r[0], r[1])
        if trusted != to_check:
            good_boy = False
            print("Got differing counts for range {0}. to_check: {1}, trusted: {2}".format(r, to_check, trusted))   

    if good_boy:
        print("All calls match")
    else:
        print("Faulty to_check endpoint")

main(sys.argv[1], sys.argv[2])