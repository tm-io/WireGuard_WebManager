from __future__ import annotations

import ipaddress
from typing import Iterable, Optional


class IPAllocationError(ValueError):
    pass


def allocate_next_ip(
    cidr: str,
    used_ips: Iterable[str],
    *,
    reserve_first_n: int = 1,
) -> Optional[str]:
    """
    CIDR で指定されたクライアント用アドレス帯から、
    `used_ips` に含まれない最初の空きIPを返す。

    - `reserve_first_n` で、先頭から何個分を予約してスキップするか指定できる
      (例: ゲートウェイ/IPAM用など)。デフォルトは1つ予約。
    - 空きがない場合は None を返す。

    Phase 2 では DB とは独立しており、`used_ips` は呼び出し側で
    peers テーブルなどから取得して渡す前提とする。
    """
    try:
        network = ipaddress.ip_network(cidr)
    except ValueError as exc:
        raise IPAllocationError(f"不正な CIDR です: {cidr}") from exc

    used_set = {ipaddress.ip_address(ip) for ip in used_ips}

    # `hosts()` はネットワークアドレス/ブロードキャストを除いたホスト部を返す
    hosts_iter = list(network.hosts())

    if reserve_first_n > 0 and reserve_first_n < len(hosts_iter):
        candidates = hosts_iter[reserve_first_n:]
    else:
        candidates = hosts_iter

    for candidate in candidates:
        if candidate not in used_set:
            return str(candidate)

    return None

