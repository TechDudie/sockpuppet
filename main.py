import argparse
import requests
import speedtest
import time

LIST_PROXIFLY = "https://raw.githubusercontent.com/proxifly/free-proxy-list/refs/heads/main/proxies/countries/{}/data.json"
LIST_FRESH = "https://raw.githubusercontent.com/vakhov/fresh-proxy-list/refs/heads/master/proxylist.json"

start_time = time.time()

def log(message: str, level="INFO"):
    elapsed_time = time.time() - start_time
    formatted_time = f"{int(elapsed_time // 3600):02}:{int((elapsed_time % 3600) // 60):02}:{int(elapsed_time % 60):02}.{int((elapsed_time % 1) * 1000):03}"
    print(f"[Hackasteis] [{formatted_time}] [{level}] {message}")

try:
    with open(".env") as f:
        API_GEOLOCATION_KEY = dict(line.strip().split("=", 1) for line in f.read().split("\n") if line.strip() and "=" in line)["API_GEOLOCATION"]
        API_GEOLOCATION = "https://api.ipgeolocation.io/ipgeo?apiKey={}&ip={}".format(API_GEOLOCATION_KEY, "{}")
except:
    log("Failed to load API key from .env file", level="WARN")

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="quick lil piece of s*** to scrape some socks5 proxies")
    parser.add_argument(
        "-c", "--country-code",
        type=str,
        help="Country code",
        default="US"
    )
    parser.add_argument(
        "-p", "--protocol",
        type=str,
        choices=["http", "socks4", "socks5"],
        help="Proxy protocol",
        default="socks5"
    )
    args = parser.parse_args()

    country_code = args.country_code
    protocol = args.protocol

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
    
    filtered_fresh = [(proxy['ip'], proxy['port']) for proxy in fresh_data if proxy.get("country_code") == country_code and proxy.get(protocol) == "1"]
    filtered_proxifly = [(proxy['ip'], proxy['port']) for proxy in proxifly_data if proxy.get("protocol") == protocol]
    proxies = list(set(filtered_fresh + filtered_proxifly))
    print(proxies)

    for i in proxies:
        ip = i[0]
        port = i[1]
        try:
            response = requests.get(API_GEOLOCATION.format(ip), timeout=5)
            response.raise_for_status()
            data = response.json()
            print(data)
        except:
            log(f"Failed to fetch geolocation for {ip}", level="WARN")