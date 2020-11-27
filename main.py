import requests
import json

ENDPOINT = "http://192.168.1.10:8545"
HEADERS = {'Content-type': 'application/json'}
TIMEOUT = 15
DEPOSIT_CONTRACT= "0x8c5fecdC472E27Bc447696F431E425D02dd46a8c" # Pyrmont
START_BLOCK = 3743587
DEPOSIT_EVENT_TOPIC = "0x649bbc62d0e31342afea4e5cd82d4049e7e1ee912fc0889aa790803be39038c5"

def send_rpc_request(method, params):
    body = {
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": 1
    }
    resp = requests.post(ENDPOINT, data = json.dumps(body), headers = HEADERS, timeout = TIMEOUT)
    return resp.json()

def get_chain_id():
    return int(send_rpc_request("eth_chainId", [])['result'], 16)

def get_network_id():
    return send_rpc_request("net_version", [])['result']

def get_block_number():
    return int(send_rpc_request("eth_blockNumber", [])['result'], 16)

def get_deposit_logs_count(start, end):
    params = [{
        "address": DEPOSIT_CONTRACT,
        "topics": [DEPOSIT_EVENT_TOPIC],
        "fromBlock": hex(start),
        "toBlock": hex(end)
    }]
    resp = send_rpc_request("eth_getLogs", params)
    return len(resp['result'])

def main():
    block = get_block_number()
    range_chunks = list(range(START_BLOCK, block, 1000))
    range_chunks = [(range_chunks[i], range_chunks[i+1]) for i in range(len(range_chunks) - 1)]
    for r in range_chunks:
        print("Chain id: ", get_chain_id())
        print("Network id", get_network_id())
        print("Got ",get_deposit_logs_count(r[0], r[1]), " deposit logs in range", r)

main()
