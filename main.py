import argparse
import copy
import multiprocessing
import os
import random
import time
import warnings
warnings.filterwarnings("ignore")
import requests

from requests.exceptions import SSLError

_VERSION = "1.0.0"
WIDTH = os.get_terminal_size().columns

LIST_PROXIFLY = "https://raw.githubusercontent.com/proxifly/free-proxy-list/refs/heads/main/proxies/countries/{}/data.json"
LIST_FRESH = "https://raw.githubusercontent.com/vakhov/fresh-proxy-list/refs/heads/master/proxylist.json"

start_time = time.time()

def log(message: str, level="INFO"):
    elapsed_time = time.time() - start_time
    formatted_time = f"{int(elapsed_time // 3600):02}:{int((elapsed_time % 3600) // 60):02}:{int(elapsed_time % 60):02}.{int((elapsed_time % 1) * 1000):03}"
    print(f"[sockscraper] [{formatted_time}] [{level}] {message}")

try:
    with open(".env") as f:
        API_GEOLOCATION_KEY = dict(line.strip().split("=", 1) for line in f.read().split("\n") if line.strip() and "=" in line)["API_GEOLOCATION"]
        API_GEOLOCATION = "https://api.ipgeolocation.io/ipgeo?apiKey={}&ip={}".format(API_GEOLOCATION_KEY, "{}")
except:
    log("Failed to load API key from .env file", level="WARN")

SPEEDTEST_DOWNLOAD = "https://raw.githubusercontent.com/TechDudie/sockscraper/refs/heads/main/static/download_{}{}".format("{}", random.randint(0, 9))

multiprocessing.freeze_support()

def speedtest(proxy, timeout=0.2, size=0):
    proxies = {
        "http": f"socks5h://{proxy}",
        "https": f"socks5h://{proxy}",
        "socks5": f"socks5h://{proxy}"
    }

    try:
        start_time = time.time()
        response = requests.get(SPEEDTEST_DOWNLOAD.format(size), proxies=proxies, timeout=timeout, stream=True)
        response.raise_for_status()
        total_size = 0
        for chunk in response.iter_content(chunk_size=1024): total_size += len(chunk)
        elapsed_time = time.time() - start_time
        download_speed = total_size / elapsed_time / 1024 / 1024
        return proxy, download_speed
    except SSLError:
        return proxy, -1
    except Exception:
        return proxy, 0

def speedtest_callback(data):
    global i, j, delta, speeds
    i += 1
    j += delta
    speeds.append(data)
    print(f"Testing proxies {str(round(j * 100)).rjust(3, ' ')}% [{('/' * int(j * (WIDTH - 22))).ljust(WIDTH - 23, ' ')}]", end="\r")

if __name__ == "__main__":
    # parser = argparse.ArgumentParser(description="quick lil piece of s*** to scrape some socks5 proxies")
    # parser.add_argument(
    #     "-c", "--country-code",
    #     type=str,
    #     help="Country code",
    #     default="US"
    # )
    # parser.add_argument(
    #     "-p", "--protocol",
    #     type=str,
    #     choices=["http", "socks4", "socks5"],
    #     help="Proxy protocol",
    #     default="socks5"
    # )
    # args = parser.parse_args()
    # 
    # country_code = args.country_code
    # protocol = args.protocol
    country_code = "US"
    protocol = "socks5"

    log(f"Ssckscraper {_VERSION} initalized")
    log("Fetching proxy data")

    try:
        proxifly_response = requests.get(LIST_PROXIFLY.format(country_code))
        proxifly_response.raise_for_status()
        proxifly_data = proxifly_response.json()

        fresh_response = requests.get(LIST_FRESH)
        fresh_response.raise_for_status()
        fresh_data = fresh_response.json()
    except:
        log("Failed to fetch data", level="ERROR")
        exit(1)
    
    filtered_fresh = [f"{proxy['ip']}:{proxy['port']}" for proxy in fresh_data if proxy.get("country_code") == country_code and proxy.get(protocol) == "1"]
    filtered_proxifly = [f"{proxy['ip']}:{proxy['port']}" for proxy in proxifly_data if proxy.get("protocol") == protocol]
    proxies = list(set(filtered_fresh + filtered_proxifly))

    # for i in proxies:
    #     ip = i[0]
    #     port = i[1]
    #     try:
    #         response = requests.get(API_GEOLOCATION.format(ip), timeout=5)
    #         response.raise_for_status()
    #         data = response.json()
    #         print(data)
    #     except:
    #         log(f"Failed to fetch geolocation for {ip}", level="WARN")

    if len(proxies) > 0:
        log(f"Benchmarking {len(proxies)} proxy servers")
    else:
        log("No proxies found", level="ERROR")
        exit()
    
    global i, j, delta, speeds
    speeds = []
    t = 2
    skibidi = True
    filtered_proxies = []
    results = []
    test_count = 1
    while skibidi:
        i = 0
        j = 0
        delta = 1 / len(proxies)
        log(f"Starting test #{test_count}, 4MB download size, {0.1 * t}s timeout at {SPEEDTEST_DOWNLOAD.format(0)}")

        pool = multiprocessing.Pool(multiprocessing.cpu_count())
        for server in proxies: pool.apply_async(speedtest, args=(server, 0.1 * t), callback=speedtest_callback)
        pool.close()
        pool.join()
        print()

        for server in speeds:
            if server[1] > -1:
                filtered_proxies.append(server[0])
            if server[1] > 0:
                results.append(server)
        
        proxies = copy.deepcopy(filtered_proxies)
        results = [proxy[0] for proxy in sorted(results, key=lambda x: x[1], reverse=True)]
        skibidi = False if len(results) > 0 else True
        if len(results) > 0:
            skibidi = False
        else:
            t += 1
            test_count += 1
            log(f"No proxies responded, raising timeout to {0.1 * t} seconds", "WARN")

    log(f"Discovered {len(results)} possible servers: {results}")
    
    speeds = []
    for i in results[:8]:
        _, speed = speedtest(i, 0.1 * t, 1)
        if speed > 0: speeds.append((i, speed))
    
    proxies = sorted(speeds, key=lambda x: x[1], reverse=True)
    
    log("Proxy data benchmarked")

    for proxy in proxies:
        log(f"Proxy {proxy[0]} - Speed: {proxy[1]:.2f}MB/s")