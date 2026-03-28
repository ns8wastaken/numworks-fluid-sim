#!/bin/bash

read -p "Do you want to continue ? (y/N) " -r

if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Installation aborted."
    [[ "$0" = "$BASH_SOURCE" ]] && exit 1 || return 1
fi

if [ ! -d "simulator" ]; then
    echo "Clonning Epsilon."
    git clone https://github.com/numworks/epsilon.git simulator --depth 1 -b version-20
    if [ $? -ne 0 ]; then
        echo "Cannot clone the Epsilon repository. Installation aborted."
        [[ "$0" = "$BASH_SOURCE" ]] && exit 1 || return 1
    fi
fi

python3 -m venv ./.venv
if [ $? -ne 0 ]; then
    echo "Cannot create the Python venv. Installation aborted."
    [[ "$0" = "$BASH_SOURCE" ]] && exit 1 || return 1
fi

source ./.venv/bin/activate
if [ $? -ne 0 ]; then
    echo "Cannot activate the Python venv. Installation aborted."
    [[ "$0" = "$BASH_SOURCE" ]] && exit 1 || return 1
fi

pip3 install lz4 pypng stringcase
if [ $? -ne 0 ]; then
    echo "The installation of the pip packages has failed. Installation aborted."
    [[ "$0" = "$BASH_SOURCE" ]] && exit 1 || return 1
fi

echo
echo "=================================="
echo "|  Installation finished! Nice!  |"
echo "=================================="
