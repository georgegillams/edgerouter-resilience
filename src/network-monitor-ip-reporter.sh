#!/bin/bash

# Function to validate IP address format (XXX.XXX.XXX.XXX where each XXX is 0-255)
validate_ip() {
    local ip=$1
    if [[ -z "$ip" ]]; then
        return 1
    fi
    
    # Check if IP matches the pattern XXX.XXX.XXX.XXX
    if [[ ! "$ip" =~ ^[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}$ ]]; then
        return 1
    fi
    
    return 0
}

# Get pppoe0 IP address
PPPOE0_IP=$(ip addr show pppoe0 2>/dev/null | grep -oE 'inet [0-9]+\.[0-9]+\.[0-9]+\.[0-9]+' | awk '{print $2}')

# Get pppoe1 IP address
PPPOE1_IP=$(ip addr show pppoe1 2>/dev/null | grep -oE 'inet [0-9]+\.[0-9]+\.[0-9]+\.[0-9]+' | awk '{print $2}')

# Report pppoe0 IP (FTTP)
if [ -n "$PPPOE0_IP" ] && validate_ip "$PPPOE0_IP"; then
    echo "ADDING FTTP IP: $PPPOE0_IP"
    curl -s "http://192.168.1.96:3001/add-fttp-ip?$PPPOE0_IP"
fi

# Report pppoe1 IP (FTTC)
if [ -n "$PPPOE1_IP" ] && validate_ip "$PPPOE1_IP"; then
    echo "ADDING FTTC IP: $PPPOE1_IP"
    curl -s "http://192.168.1.96:3001/add-fttc-ip?$PPPOE1_IP" > /dev/null
fi

