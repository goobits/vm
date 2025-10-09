#!/bin/bash
# Collect all metrics in one SSH call and emit JSON

cpu_percent=$(top -bn1 | grep "Cpu(s)" | awk '{print $2}' | sed 's/%us,//')
memory_used_kb=$(free | grep Mem | awk '{print $3}')
memory_total_kb=$(free | grep Mem | awk '{print $2}')
disk_info=$(df -BG / | tail -1)
disk_used_gb=$(echo $disk_info | awk '{print $3}' | sed 's/G//')
disk_total_gb=$(echo $disk_info | awk '{print $2}' | sed 's/G//')
uptime_str=$(uptime -p)

# Convert memory to MB
memory_used_mb=$((memory_used_kb / 1024))
memory_total_mb=$((memory_total_kb / 1024))

# Check systemd services
postgres_status="false"
redis_status="false"
mongodb_status="false"

if systemctl is-active --quiet postgresql 2>/dev/null; then
    postgres_status="true"
fi

if systemctl is-active --quiet redis-server 2>/dev/null; then
    redis_status="true"
fi

if systemctl is-active --quiet mongodb 2>/dev/null; then
    mongodb_status="true"
fi

# Emit JSON
cat <<EOF
{
  "cpu_percent": ${cpu_percent:-0},
  "memory_used_mb": ${memory_used_mb:-0},
  "memory_limit_mb": ${memory_total_mb:-0},
  "disk_used_gb": ${disk_used_gb:-0},
  "disk_total_gb": ${disk_total_gb:-0},
  "uptime": "${uptime_str}",
  "services": [
    {"name": "postgresql", "is_running": ${postgres_status}},
    {"name": "redis", "is_running": ${redis_status}},
    {"name": "mongodb", "is_running": ${mongodb_status}}
  ]
}
EOF