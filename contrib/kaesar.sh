#!/bin/sh
export LC_ALL="en_US.UTF-8"

bindir=bin/utils

# Compute our working directory in an extremely defensive manner
SCRIPT_DIR="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd -P)"
# NOTE: We need to remember the *actual* KAESAR_DIR, not the relocalized version in /tmp...
export KAESAR_DIR="${KAESAR_DIR:-${SCRIPT_DIR}}"
UNPACK_DIR="${KAESAR_DIR%/*}"

# We rely on starting from our working directory, and it needs to be set, sane and absolute.
cd "${KAESAR_DIR:-/dev/null}" || exit

KAESAR_SET_FRAMEBUFFER_DEPTH=1
KAESAR_CONVERT_DICTIONARIES=1

# To make USBMS behave, relocalize ourselves outside of onboard. Additionally,
# this is used by Kaesar to detect if the original script has changed after
# an update (requiring a complete restart from the parent launcher).
if [ "${SCRIPT_DIR}" != "/tmp" ]; then
    cp -pf "${0}" "/tmp/kaesar.sh"
    chmod 777 "/tmp/kaesar.sh"
    exec "/tmp/kaesar.sh" "$@"
fi

# Attempt to switch to a sensible CPUFreq governor when that's not already the case...
# Swap every CPU at once if available
if [ -d "/sys/devices/system/cpu/cpufreq/policy0" ]; then
    CPUFREQ_SYSFS_PATH="/sys/devices/system/cpu/cpufreq/policy0"
else
    CPUFREQ_SYSFS_PATH="/sys/devices/system/cpu/cpu0/cpufreq"
fi
IFS= read -r current_cpufreq_gov <"${CPUFREQ_SYSFS_PATH}/scaling_governor"
# NOTE: What's available depends on the HW, so, we'll have to take it step by step...
#       Roughly follow Nickel's behavior (which prefers interactive), and prefer interactive, then ondemand, and finally conservative/dvfs.
if [ "${current_cpufreq_gov}" != "interactive" ]; then
    if grep -q "interactive" "${CPUFREQ_SYSFS_PATH}/scaling_available_governors"; then
        ORIG_CPUFREQ_GOV="${current_cpufreq_gov}"
        echo "interactive" >"${CPUFREQ_SYSFS_PATH}/scaling_governor"
    elif [ "${current_cpufreq_gov}" != "ondemand" ]; then
        if grep -q "ondemand" "${CPUFREQ_SYSFS_PATH}/scaling_available_governors"; then
            # NOTE: This should never really happen: every kernel that supports ondemand already supports interactive ;).
            #       They were both introduced on Mk. 6
            ORIG_CPUFREQ_GOV="${current_cpufreq_gov}"
            echo "ondemand" >"${CPUFREQ_SYSFS_PATH}/scaling_governor"
        elif [ -e "/sys/devices/platform/mxc_dvfs_core.0/enable" ]; then
            # The rest of this block assumes userspace is available...
            if grep -q "userspace" "${CPUFREQ_SYSFS_PATH}/scaling_available_governors"; then
                ORIG_CPUFREQ_GOV="${current_cpufreq_gov}"
                export CPUFREQ_DVFS="true"

                # If we can use conservative, do so, but we'll tweak it a bit to make it somewhat useful given our load patterns...
                # We unfortunately don't have any better choices on those kernels,
                # the only other governors available are powersave & performance (c.f., #4114)...
                if grep -q "conservative" "${CPUFREQ_SYSFS_PATH}/scaling_available_governors"; then
                    export CPUFREQ_CONSERVATIVE="true"
                    echo "conservative" >"${CPUFREQ_SYSFS_PATH}/scaling_governor"
                    # NOTE: The knobs survive a governor switch, which is why we do this now ;).
                    echo "2" >"/sys/devices/system/cpu/cpufreq/conservative/sampling_down_factor"
                    echo "50" >"/sys/devices/system/cpu/cpufreq/conservative/freq_step"
                    echo "11" >"/sys/devices/system/cpu/cpufreq/conservative/down_threshold"
                    echo "12" >"/sys/devices/system/cpu/cpufreq/conservative/up_threshold"
                    # NOTE: The default sampling_rate is a bit high for my tastes,
                    #       but it unfortunately defaults to its lowest possible setting...
                fi

                # NOTE: Now, here comes the freaky stuff... On a H2O, DVFS is only enabled when Wi-Fi is *on*.
                #       When it's off, DVFS is off, which pegs the CPU @ max clock given that DVFS means the userspace governor.
                #       The flip may originally have been switched by the sdio_wifi_pwr module itself,
                #       via ntx_wifi_power_ctrl @ arch/arm/mach-mx5/mx50_ntx_io.c (which is also the CM_WIFI_CTRL (208) ntx_io ioctl),
                #       but the code in the published H2O kernel sources actually does the reverse, and is commented out ;).
                #       It is now entirely handled by Nickel, right *before* loading/unloading that module.
                #       (There's also a bug(?) where that behavior is inverted for the *first* Wi-Fi session after a cold boot...)
                if grep -q "^sdio_wifi_pwr " "/proc/modules"; then
                    # Wi-Fi is enabled, make sure DVFS is on
                    echo "userspace" >"${CPUFREQ_SYSFS_PATH}/scaling_governor"
                    echo "1" >"/sys/devices/platform/mxc_dvfs_core.0/enable"
                else
                    # Wi-Fi is disabled, make sure DVFS is off
                    echo "0" >"/sys/devices/platform/mxc_dvfs_core.0/enable"

                    # Switch to conservative to avoid being stuck at max clock if we can...
                    if [ -n "${CPUFREQ_CONSERVATIVE}" ]; then
                        echo "conservative" >"${CPUFREQ_SYSFS_PATH}/scaling_governor"
                    else
                        # Otherwise, we'll be pegged at max clock...
                        echo "userspace" >"${CPUFREQ_SYSFS_PATH}/scaling_governor"
                        # The kernel should already be taking care of that...
                        cat "${CPUFREQ_SYSFS_PATH}/scaling_max_freq" >"${CPUFREQ_SYSFS_PATH}/scaling_setspeed"
                    fi
                fi
            fi
        fi
    fi
fi

# update to new version from OTA directory
ko_update_check() {
    NEWUPDATE="${KAESAR_DIR}/ota/kaesar.updated.tar"
    INSTALLED="${KAESAR_DIR}/ota/kaesar.installed.tar"
    if [ -f "${NEWUPDATE}" ]; then
        # Clear screen to delete UI leftovers
        "$bindir"/fbink --cls
        "$bindir"/fbink -q -y -7 -pmh "Updating Kaesar"
        # Keep a copy of the old manifest for cleaning leftovers later.
        cp "${KAESAR_DIR}/ota/package.index" /tmp/
        # Setup the FBInk daemon
        export FBINK_NAMED_PIPE="/tmp/kaesar.fbink"
        rm -f "${FBINK_NAMED_PIPE}"
        # We'll want to use REAGL on sunxi, because AUTO is slow, and fast merges are extremely broken outside of REAGL...
        eval "$($bindir/fbink -e | tr ';' '\n' | grep -e isSunxi | tr '\n' ';')"
        # shellcheck disable=SC2154
        if [ "${isSunxi}" = "1" ]; then
            PBAR_WFM="REAGL"
        else
            PBAR_WFM="AUTO"
        fi
        FBINK_PID="$($bindir/fbink --daemon 1 %KAESAR% -q -y -6 -P 0 -W ${PBAR_WFM})"
        # NOTE: See frontend/ui/otamanager.lua for a few more details on how we squeeze a percentage out of tar's checkpoint feature
        # NOTE: %B should always be 512 in our case, so let stat do part of the maths for us instead of using %s ;).
        FILESIZE="$(stat -c %b "${NEWUPDATE}")"
        BLOCKS="$((FILESIZE / 20))"
        export CPOINTS="$((BLOCKS / 100))"
        # shellcheck disable=SC2016
        ./tar xf "${NEWUPDATE}" --strip-components=1 --no-same-permissions --no-same-owner --checkpoint="${CPOINTS}" --checkpoint-action=exec='printf "%s" $((TAR_CHECKPOINT / CPOINTS)) > ${FBINK_NAMED_PIPE}'
        fail=$?
        kill -TERM "${FBINK_PID}"
        # Cleanup behind us...
        if [ "${fail}" -eq 0 ]; then
            mv "${NEWUPDATE}" "${INSTALLED}"
            # Cleanup leftovers from previous install.
            (cd "${UNPACK_DIR}" && grep -xvFf "${KAESAR_DIR}/ota/package.index" /tmp/package.index | xargs -r rm -vf)
            "$bindir"/fbink -q -y -6 -pm "Update successful :)"
            "$bindir"/fbink -q -y -5 -pm "Kaesar will start momentarily . . ."

            # Warn if the startup script has been updated...
            if [ "$(md5sum "/tmp/kaesar.sh" | cut -f1 -d' ')" != "$(md5sum "${KAESAR_DIR}/kaesar.sh" | cut -f1 -d' ')" ]; then
                "$bindir"/fbink -q -pmMh "Update contains a startup script update!"
            fi
        else
            # Uh oh...
            "$bindir"/fbink -q -y -6 -pmh "Update failed :("
            "$bindir"/fbink -q -y -5 -pm "Kaesar may fail to function properly!"
        fi
        rm -f /tmp/package.index "${NEWUPDATE}" # always purge newupdate to prevent update loops
        unset CPOINTS FBINK_NAMED_PIPE
        unset BLOCKS FILESIZE FBINK_PID
        # Ensure everything is flushed to disk before we restart. This *will* stall for a while on slow storage!
        sync
    fi
}
# NOTE: Keep doing an initial update check, in addition to one during the restart loop, so we can pickup potential updates of this very script...
ko_update_check
# If an update happened, and was successful, reexec
if [ -n "${fail}" ] && [ "${fail}" -eq 0 ]; then
    # By now, we know we're in the right directory, and our script name is pretty much set in stone, so we can forgo using $0
    exec ./kaesar.sh "${@}"
fi

# export external font directory
export EXT_FONT_DIR="/mnt/onboard/fonts"

# Quick'n dirty way of checking if we were started while Nickel was running (e.g., KFMon),
# or from another launcher entirely, outside of Nickel (e.g., KSM).
VIA_NICKEL="false"
if pkill -0 nickel; then
    VIA_NICKEL="true"
fi
# NOTE: Do not delete this line because KSM detects newer versions of Kaesar by the presence of the phrase 'from_nickel'.

if [ "${VIA_NICKEL}" = "true" ]; then
    # Detect if we were started from KFMon
    FROM_KFMON="false"
    if pkill -0 kfmon; then
        # That's a start, now check if KFMon truly is our parent...
        if [ "$(pidof -s kfmon)" -eq "${PPID}" ]; then
            FROM_KFMON="true"
        fi
    fi

    # Check if Nickel is our parent...
    FROM_NICKEL="false"
    if [ -n "${NICKEL_HOME}" ]; then
        FROM_NICKEL="true"
    fi

    # If we were spawned outside of Nickel, we'll need a few extra bits from its own env...
    if [ "${FROM_NICKEL}" = "false" ]; then
        # Siphon a few things from nickel's env (namely, stuff exported by rcS *after* on-animator.sh has been launched)...
        # shellcheck disable=SC2046
        export $(grep -s -E -e '^(DBUS_SESSION_BUS_ADDRESS|NICKEL_HOME|WIFI_MODULE|LANG|INTERFACE)=' "/proc/$(pidof -s nickel)/environ")
        # NOTE: Quoted variant, w/ the busybox RS quirk (c.f., https://unix.stackexchange.com/a/125146):
        #eval "$(awk -v 'RS="\0"' '/^(DBUS_SESSION_BUS_ADDRESS|NICKEL_HOME|WIFI_MODULE|LANG|INTERFACE)=/{gsub("\047", "\047\\\047\047"); print "export \047" $0 "\047"}' "/proc/$(pidof -s nickel)/environ")"
    fi

    # If bluetooth is enabled, kill it.
    if [ -e "/sys/devices/platform/bt/rfkill/rfkill0/state" ]; then
        # That's on sunxi, at least
        IFS= read -r bt_state <"/sys/devices/platform/bt/rfkill/rfkill0/state"
        if [ "${bt_state}" = "1" ]; then
            echo "0" >"/sys/devices/platform/bt/rfkill/rfkill0/state"

            # Power the chip down
            ioctl -q -v 0 /dev/ntx_io 208
        fi
    fi
    if grep -q "^sdio_bt_pwr " "/proc/modules"; then
        # And that's on NXP SoCs
        rmmod sdio_bt_pwr
    fi

    # Flush disks, might help avoid trashing nickel's DB...
    sync
    # And we can now stop the full Kobo software stack
    # NOTE: We don't need to kill KFMon, it's smart enough not to allow running anything else while we're up
    # NOTE: We kill Nickel's master dhcpcd daemon on purpose,
    #       as we want to be able to use our own per-if processes w/ custom args later on.
    #       A SIGTERM does not break anything, it'll just prevent automatic lease renewal until the time
    #       Kaesar actually sets the if up itself (i.e., it'll do)...
    killall -q -TERM nickel hindenburg sickel fickel strickel fontickel adobehost foxitpdf iink dhcpcd-dbus dhcpcd bluealsa bluetoothd fmon

    # Wait for Nickel to die... (oh, procps with killall -w, how I miss you...)
    kill_timeout=0
    while pkill -0 nickel; do
        # Stop waiting after 4s
        if [ ${kill_timeout} -ge 15 ]; then
            break
        fi
        usleep 250000
        kill_timeout=$((kill_timeout + 1))
    done
    # Remove Nickel's FIFO to avoid udev & udhcpc scripts hanging on open() on it...
    #rm -f /tmp/nickel-hardware-status

    # We don't need to grab input devices (unless MiniClock is running, in which case that neatly inhibits it while we run).
    if [ ! -d "/tmp/MiniClock" ]; then
        export KO_DONT_GRAB_INPUT="true"
    fi
fi

# check whether PLATFORM & PRODUCT have a value assigned by rcS
if [ -z "${PRODUCT}" ]; then
    # shellcheck disable=SC2046
    export $(grep -s -e '^PRODUCT=' "/proc/$(pidof -s udevd)/environ")
fi

if [ -z "${PRODUCT}" ]; then
    PRODUCT="$(/bin/kobo_config.sh 2>/dev/null)"
    export PRODUCT
fi

# PLATFORM is used in kaesar for the path to the Wi-Fi drivers (as well as when restarting nickel)
if [ -z "${PLATFORM}" ]; then
    # shellcheck disable=SC2046
    export $(grep -s -e '^PLATFORM=' "/proc/$(pidof -s udevd)/environ")
fi

if [ -z "${PLATFORM}" ]; then
    PLATFORM="freescale"
    if dd if="/dev/mmcblk0" bs=512 skip=1024 count=1 | grep -q "HW CONFIG"; then
        CPU="$(ntx_hwconfig -s -p /dev/mmcblk0 CPU 2>/dev/null)"
        PLATFORM="${CPU}-ntx"
    fi

    if [ "${PLATFORM}" != "freescale" ] && [ ! -e "/etc/u-boot/${PLATFORM}/u-boot.mmc" ]; then
        PLATFORM="ntx508"
    fi
    export PLATFORM
fi

# Make sure we have a sane-ish INTERFACE env var set...
if [ -z "${INTERFACE}" ]; then
    # That's what we used to hardcode anyway
    INTERFACE="eth0"
    export INTERFACE
fi

# We'll enforce UR in ko_do_fbdepth, so make sure further FBInk usage (USBMS)
# will also enforce UR... (Only actually meaningful on sunxi).
if [ "${PLATFORM}" = "b300-ntx" ]; then
    export FBINK_FORCE_ROTA=0
    # On sunxi, non-REAGL waveform modes suffer from weird merging quirks...
    FBINK_WFM="REAGL"
    # And we also cannot use batched updates for the crash screen, as buffers are private,
    # so each invocation essentially draws in a different buffer...
    FBINK_BATCH_FLAG=""
    # Same idea for backgroundless...
    FBINK_BGLESS_FLAG="-B GRAY9"
    # It also means we need explicit background padding in the OT codepath...
    FBINK_OT_PADDING=",padding=BOTH"

    # Make sure we poke the right input device
    KOBO_TS_INPUT="/dev/input/by-path/platform-0-0010-event"
else
    FBINK_WFM="GL16"
    FBINK_BATCH_FLAG="-b"
    FBINK_BGLESS_FLAG="-O"
    FBINK_OT_PADDING=""
    KOBO_TS_INPUT="/dev/input/event1"
fi

# We'll want to ensure Portrait rotation to allow us to use faster blitting codepaths @ 8bpp,
# so remember the current one before fbdepth does its thing.
IFS= read -r ORIG_FB_ROTA <"/sys/class/graphics/fb0/rotate"
echo "Original fb rotation is set @ ${ORIG_FB_ROTA}" >>crash.log 2>&1

# In the same vein, swap to 8bpp,
# because 16bpp is the worst idea in the history of time, as RGB565 is generally a PITA without hardware blitting,
# and 32bpp usually gains us nothing except a performance hit (we're not Qt5 with its QPainter constraints).
# The reduced size & complexity should hopefully make things snappier,
# (and hopefully prevent the JIT from going crazy on high-density screens...).
# NOTE: Even though both pickel & Nickel appear to restore their preferred fb setup, we'll have to do it ourselves,
#       as they fail to flip the greyscale flag properly. Plus, we get to play nice with every launch method that way.
#       So, remember the current bitdepth, so we can restore it on exit.
IFS= read -r ORIG_FB_BPP <"/sys/class/graphics/fb0/bits_per_pixel"
echo "Original fb bitdepth is set @ ${ORIG_FB_BPP}bpp" >>crash.log 2>&1
# Sanity check...
case "${ORIG_FB_BPP}" in
    8) ;;
    16) ;;
    32) ;;
    *)
        # Uh oh? Don't do anything...
        unset ORIG_FB_BPP
        ;;
esac

# The actual swap is done in a function, because we can disable it in the Developer settings, and we want to honor it on restart.
ko_do_fbdepth() {
    # On sunxi, the fb state is meaningless, and the minimal disp fb doesn't actually support 8bpp anyway...
    if [ "${PLATFORM}" = "b300-ntx" ]; then
        # NOTE: The fb state is *completely* meaningless on this platform.
        #       This is effectively a noop, we're just keeping it for logging purposes...
        echo "Making sure that rotation is set to Portrait" >>crash.log 2>&1
        "$bindir"/fbdepth -R UR >>crash.log 2>&1
        # We haven't actually done anything, so don't do anything on exit either ;).
        unset ORIG_FB_BPP

        return
    fi

    # On color panels, we target 32bpp for, well, color, and sane addressing (it also happens to be their default) ;o).
    eval "$($bindir/fbink -e | tr ';' '\n' | grep -e hasColorPanel | tr '\n' ';')"
    # shellcheck disable=SC2154
    if [ "${hasColorPanel}" = "1" ]; then
        # If color rendering has been disabled by the user, switch to 8bpp to completely skip CFA processing
        if grep -q '\["color_rendering"\] = false' 'settings.reader.lua' 2>/dev/null; then
            echo "Switching fb bitdepth to 8bpp (to disable CFA) & rotation to Portrait" >>crash.log 2>&1
            "$bindir"/fbdepth -d 8 -R UR >>crash.log 2>&1
        else
            echo "Switching fb bitdepth to 32bpp & rotation to Portrait" >>crash.log 2>&1
            "$bindir"/fbdepth -d 32 -R UR >>crash.log 2>&1
        fi

        return
    fi

    # Check if the swap has been disabled...
    if grep -q '\["dev_startup_no_fbdepth"\] = true' 'settings.reader.lua' 2>/dev/null; then
        # Swap back to the original bitdepth (in case this was a restart)
        if [ -n "${ORIG_FB_BPP}" ]; then
            # Unless we're a Forma/Libra, don't even bother to swap rotation if the fb is @ 16bpp, because RGB565 is terrible anyways,
            # so there's no faster codepath to achieve, and running in Portrait @ 16bpp might actually be broken on some setups...
            if [ "${ORIG_FB_BPP}" -eq "16" ] && [ "${PRODUCT}" != "frost" ] && [ "${PRODUCT}" != "storm" ]; then
                echo "Making sure we're using the original fb bitdepth @ ${ORIG_FB_BPP}bpp & rotation @ ${ORIG_FB_ROTA}" >>crash.log 2>&1
                "$bindir"/fbdepth -d "${ORIG_FB_BPP}" -r "${ORIG_FB_ROTA}" >>crash.log 2>&1
            else
                echo "Making sure we're using the original fb bitdepth @ ${ORIG_FB_BPP}bpp, and that rotation is set to Portrait" >>crash.log 2>&1
                "$bindir"/fbdepth -d "${ORIG_FB_BPP}" -R UR >>crash.log 2>&1
            fi
        fi
    else
        # Swap to 8bpp if things looke sane
        if [ -n "${ORIG_FB_BPP}" ]; then
            echo "Switching fb bitdepth to 8bpp & rotation to Portrait" >>crash.log 2>&1
            "$bindir"/fbdepth -d 8 -R UR >>crash.log 2>&1
        fi
    fi
}

# Ensure we start with a valid nameserver in resolv.conf, otherwise we're stuck with broken name resolution (#6421, #6424).
# Fun fact: this wouldn't be necessary if Kobo were using a non-prehistoric glibc... (it was fixed in glibc 2.26).
ko_do_dns() {
    # If there aren't any servers listed, append CloudFlare's
    if ! grep -q '^nameserver' "/etc/resolv.conf"; then
        echo "# Added by Kaesar because your setup is broken" >>"/etc/resolv.conf"
        echo "nameserver 1.1.1.1" >>"/etc/resolv.conf"
    fi
}

# Remount the SD card RW if it's inserted and currently RO
if awk '$4~/(^|,)ro($|,)/' /proc/mounts | grep ' /mnt/sd '; then
    mount -o remount,rw /mnt/sd
fi

################################# LEDS #################################
if [ -e /sys/class/leds/LED ] ; then
	LEDS_INTERFACE=/sys/class/leds/LED/brightness
	STANDARD_LEDS=1
elif [ -e /sys/class/leds/GLED ] ; then
	LEDS_INTERFACE=/sys/class/leds/GLED/brightness
	STANDARD_LEDS=1
elif [ -e /sys/class/leds/bd71828-green-led ] ; then
	LEDS_INTERFACE=/sys/class/leds/bd71828-green-led/brightness
	STANDARD_LEDS=1
elif [ -e /sys/devices/platform/ntx_led/lit ] ; then
	LEDS_INTERFACE=/sys/devices/platform/ntx_led/lit
	STANDARD_LEDS=0
elif [ -e /sys/devices/platform/pmic_light.1/lit ] ; then
	LEDS_INTERFACE=/sys/devices/platform/pmic_light.1/lit
	STANDARD_LEDS=0
fi

# Turn off the LEDs
if [ "$STANDARD_LEDS" -eq 1 ] ; then
	echo 0 > "$LEDS_INTERFACE"
else
	# https://www.tablix.org/~avian/blog/archives/2013/03/blinken_kindle/
	for ch in 3 4 5; do
		echo "ch ${ch}" > "$LEDS_INTERFACE"
		echo "cur 1" > "$LEDS_INTERFACE"
		echo "dc 0" > "$LEDS_INTERFACE"
	done
fi
################################# LEDS #################################

# Define environment variables used by `scripts/usb-*.sh`
KOBO_TAG=/mnt/onboard/.kobo/version
if [ -e "$KOBO_TAG" ] ; then
	SERIAL_NUMBER=$(cut -f 1 -d ',' "$KOBO_TAG")
	FIRMWARE_VERSION=$(cut -f 3 -d ',' "$KOBO_TAG")
	MODEL_NUMBER=$(cut -f 6 -d ',' "$KOBO_TAG" | sed -e 's/^[0-]*//')

	# This is a combination of the information given in `FBInk/fbink_device_id.c`
	# and `calibre/src/calibre/devices/kobo/driver.py`.
	case "$MODEL_NUMBER" in
		3[12]0)  PRODUCT_ID=0x4163 ;; # Touch A/B, Touch C
		330)     PRODUCT_ID=0x4173 ;; # Glo
		340)     PRODUCT_ID=0x4183 ;; # Mini
		350)     PRODUCT_ID=0x4193 ;; # Aura HD
		360)     PRODUCT_ID=0x4203 ;; # Aura
		370)     PRODUCT_ID=0x4213 ;; # Aura H₂O
		371)     PRODUCT_ID=0x4223 ;; # Glo HD
		372)     PRODUCT_ID=0x4224 ;; # Touch 2.0
		373|381) PRODUCT_ID=0x4225 ;; # Aura ONE, Aura ONE Limited Edition
		374)     PRODUCT_ID=0x4227 ;; # Aura H₂O Edition 2
		375)     PRODUCT_ID=0x4226 ;; # Aura Edition 2
		376)     PRODUCT_ID=0x4228 ;; # Clara HD
		377|380) PRODUCT_ID=0x4229 ;; # Forma, Forma 32GB
		384)     PRODUCT_ID=0x4232 ;; # Libra H₂O
		382)     PRODUCT_ID=0x4230 ;; # Nia
		387)     PRODUCT_ID=0x4233 ;; # Elipsa
		383)     PRODUCT_ID=0x4231 ;; # Sage
		388)     PRODUCT_ID=0x4234 ;; # Libra 2
		386)     PRODUCT_ID=0x4235 ;; # Clara 2E
		389)     PRODUCT_ID=0x4236 ;; # Elipsa 2E
		390)     PRODUCT_ID=0x4237 ;; # Libra Colour
		393)     PRODUCT_ID=0x4238 ;; # Clara Colour
		391)     PRODUCT_ID=0x4239 ;; # Clara BW
		*)       PRODUCT_ID=0x6666 ;;
	esac

	export SERIAL_NUMBER FIRMWARE_VERSION MODEL_NUMBER PRODUCT_ID
fi

export LD_LIBRARY_PATH="libs:${LD_LIBRARY_PATH}"

[ -e info.log ] && [ "$(stat -c '%s' info.log)" -gt $((1<<18)) ] && mv info.log archive.log

[ "$KAESAR_CONVERT_DICTIONARIES" ] && find -L dictionaries -name '*.ifo' -exec ./convert-dictionary.sh {} \;

# we keep at most 500KB worth of crash log
if [ -e crash.log ]; then
    tail -c 500000 crash.log >crash.log.new
    mv -f crash.log.new crash.log
fi

CRASH_COUNT=0
CRASH_TS=0
CRASH_PREV_TS=0
# List of supported special return codes
KO_RC_RESTART_TO_KAESAR=85
KO_RC_REBOOT=87
KO_RC_HALT=88
# Because we *want* an initial fbdepth pass ;).
RETURN_VALUE=${KO_RC_RESTART_TO_KAESAR}
while [ ${RETURN_VALUE} -ne 0 ]; do
    if [ ${RETURN_VALUE} -eq ${KO_RC_RESTART_TO_KAESAR} ]; then
        # Do an update check now, so we can actually update Kaesar via the "Restart Kaesar" menu entry ;).
        ko_update_check
        # Do or double-check the fb depth switch, or restore original bitdepth if requested
        ko_do_fbdepth
        # Make sure we have a sane resolv.conf
        ko_do_dns
    fi

    LIBC_FATAL_STDERR_=1 ./kaesar >> info.log 2>&1
    RETURN_VALUE=$?

    # Did we crash?
    if [ ${RETURN_VALUE} -ne 0 ] && [ ${RETURN_VALUE} -ne ${KO_RC_RESTART_TO_KAESAR} ] && [ ${RETURN_VALUE} -ne ${KO_RC_REBOOT} ] && [ ${RETURN_VALUE} -ne ${KO_RC_HALT} ]; then
        # Increment the crash counter
        CRASH_COUNT=$((CRASH_COUNT + 1))
        CRASH_TS=$(date +'%s')
        # Reset it to a first crash if it's been a while since our last crash...
        if [ $((CRASH_TS - CRASH_PREV_TS)) -ge 20 ]; then
            CRASH_COUNT=1
        fi

        # Check if the user requested to always abort on crash
        if grep -q '\["dev_abort_on_crash"\] = true' 'settings.reader.lua' 2>/dev/null; then
            ALWAYS_ABORT="true"
            # In which case, make sure we pause on *every* crash
            CRASH_COUNT=1
        else
            ALWAYS_ABORT="false"
        fi

        # Show a fancy bomb on screen
        viewWidth=600
        viewHeight=800
        FONTH=16
        eval "$($bindir/fbink -e | tr ';' '\n' | grep -e viewWidth -e viewHeight -e FONTH | tr '\n' ';')"
        # Compute margins & sizes relative to the screen's resolution, so we end up with a similar layout, no matter the device.
        # Height @ ~56.7%, w/ a margin worth 1.5 lines
        bombHeight=$((viewHeight / 2 + viewHeight / 15))
        bombMargin=$((FONTH + FONTH / 2))
        # Start with a big grey screen of death, and our friendly old school crash icon ;)
        # U+1F4A3, the hard way, because we can't use \u or \U escape sequences...
        # shellcheck disable=SC2039,SC3003,SC2086
        "$bindir"/fbink -q ${FBINK_BATCH_FLAG} -c -B GRAY9 -m -t regular=./fonts/noto/NotoSans-Regular.ttf,px=${bombHeight},top=${bombMargin} -W ${FBINK_WFM} -- $'\xf0\x9f\x92\xa3'
        # With a little notice at the top of the screen, on a big grey screen of death ;).
        # shellcheck disable=SC2086
        "$bindir"/fbink -q ${FBINK_BATCH_FLAG} ${FBINK_BGLESS_FLAG} -m -y 1 -W ${FBINK_WFM} -- "Don't Panic! (Crash n°${CRASH_COUNT} -> ${RETURN_VALUE})"
        if [ ${CRASH_COUNT} -eq 1 ]; then
            # Warn that we're waiting on a tap to continue...
            # shellcheck disable=SC2086
            "$bindir"/fbink -q ${FBINK_BATCH_FLAG} ${FBINK_BGLESS_FLAG} -m -y 2 -W ${FBINK_WFM} -- "Tap the screen to continue."
        fi
        # And then print the tail end of the log on the bottom of the screen...
        crashLog="$(tail -n 25 crash.log | sed -e 's/\t/    /g')"
        # The idea for the margins being to leave enough room for an fbink -Z bar, small horizontal margins, and a font size based on what 6pt looked like @ 265dpi
        # shellcheck disable=SC2086
        "$bindir"/fbink -q ${FBINK_BATCH_FLAG} ${FBINK_BGLESS_FLAG} -t regular=./fonts/sourcecode/SourceCodeVariable-Roman.otf,top=$((viewHeight / 2 + FONTH * 2 + FONTH / 2)),left=$((viewWidth / 60)),right=$((viewWidth / 60)),px=$((viewHeight / 64))${FBINK_OT_PADDING} -W ${FBINK_WFM} -- "${crashLog}"
        if [ "${PLATFORM}" != "b300-ntx" ]; then
            # So far, we hadn't triggered an actual screen refresh, do that now, to make sure everything is bundled in a single flashing refresh.
            "$bindir"/fbink -q -f -s
        fi
        # Cue a lemming's faceplant sound effect!

        {
            echo "!!!!"
            echo "Uh oh, something went awry... (Crash n°${CRASH_COUNT}: $(date +'%x @ %X'))"
            echo "Running FW $(cut -f3 -d',' /mnt/onboard/.kobo/version) on Linux $(uname -r) ($(uname -v))"
        } >>crash.log 2>&1
        if [ ${CRASH_COUNT} -lt 5 ] && [ "${ALWAYS_ABORT}" = "false" ]; then
            echo "Attempting to restart Kaesar . . ." >>crash.log 2>&1
            echo "!!!!" >>crash.log 2>&1
        fi

        # Pause a bit if it's the first crash in a while, so that it actually has a chance of getting noticed ;).
        if [ ${CRASH_COUNT} -eq 1 ]; then
            # NOTE: We don't actually care about what read read, we're just using it as a fancy sleep ;).
            #       i.e., we pause either until the 15s timeout, or until the user touches the screen.
            # shellcheck disable=SC2039,SC3045
            read -r -t 15 <"${KOBO_TS_INPUT}"
        fi
        # Cycle the last crash timestamp
        CRASH_PREV_TS=${CRASH_TS}

        # But if we've crashed more than 5 consecutive times, exit, because we wouldn't want to be stuck in a loop...
        # NOTE: No need to check for ALWAYS_ABORT, CRASH_COUNT will always be 1 when it's true ;).
        if [ ${CRASH_COUNT} -ge 5 ]; then
            echo "Too many consecutive crashes, aborting . . ." >>crash.log 2>&1
            echo "!!!! ! !!!!" >>crash.log 2>&1
            break
        fi

        # If the user requested to always abort on crash, do so.
        if [ "${ALWAYS_ABORT}" = "true" ]; then
            echo "Aborting . . ." >>crash.log 2>&1
            echo "!!!! ! !!!!" >>crash.log 2>&1
            break
        fi
    else
        # Reset the crash counter if that was a sane exit/restart
        CRASH_COUNT=0
    fi

    # Did we request a reboot/shutdown?
    if [[ ${RETURN_VALUE} == ${KO_RC_HALT} || ${RETURN_VALUE} == ${KO_RC_REBOOT} ]]; then
        break
    fi
done

# If we requested a reboot/shutdown, no need to bother with this...
if [[ ${RETURN_VALUE} != ${KO_RC_HALT} && ${RETURN_VALUE} != ${KO_RC_REBOOT} ]]; then
    # Restore original fb bitdepth if need be...
    # Since we also (almost) always enforce Portrait, we also have to restore the original rotation no matter what ;).
    if [ -n "${ORIG_FB_BPP}" ]; then
        echo "Restoring original fb bitdepth @ ${ORIG_FB_BPP}bpp & rotation @ ${ORIG_FB_ROTA}" >>crash.log 2>&1
        "$bindir"/fbdepth -d "${ORIG_FB_BPP}" -r "${ORIG_FB_ROTA}" >>crash.log 2>&1
    else
        echo "Restoring original fb rotation @ ${ORIG_FB_ROTA}" >>crash.log 2>&1
        "$bindir"/fbdepth -r "${ORIG_FB_ROTA}" >>crash.log 2>&1
    fi

    # Restore original CPUFreq governor if need be...
    if [ -n "${ORIG_CPUFREQ_GOV}" ]; then
        echo "${ORIG_CPUFREQ_GOV}" >"${CPUFREQ_SYSFS_PATH}/scaling_governor"

        # NOTE: Leave DVFS alone, it'll be handled by Nickel if necessary.
    fi

    if [ "${VIA_NICKEL}" = "true" ]; then
        if [ "${FROM_KFMON}" = "true" ]; then
            # KFMon is the only launcher that has a toggle to either reboot or restart Nickel on exit
            if grep -q "reboot_on_exit=false" "/mnt/onboard/.adds/kfmon/config/kaesar.ini" 2>/dev/null; then
                # KFMon asked us to restart nickel on exit (default since KFMon 0.9.5)
                ./nickel.sh &
            else
                # KFMon asked us to restart the device on exit
                /sbin/reboot
            fi
        else
            # Otherwise, just restart Nickel
            ./nickel.sh &
        fi
    else
        # if we were called from advboot then we must reboot to go to the menu
        # NOTE: This is actually achieved by checking if KSM or a KSM-related script is running:
        #       This might lead to false-positives if you use neither KSM nor advboot to launch Kaesar *without nickel running*.
        if ! pkill -0 -f kbmenu; then
            /sbin/reboot
        fi
    fi
else
    if [ "${VIA_NICKEL}" = "false" ]; then
        if pkill -0 -f kbmenu; then
            # If we were started by KSM and requested an exit, attempt to *NOT* exit the script,
            # so as not to re-enter KSM at all, to make sure the device powers off with our own ScreenSaver displayed.
            # NOTE: This might not be fool-proof, as a poweroff might take longer than that,
            #       or we might be interrupted early by signals.
            sleep 10
        fi
    fi
fi

# Wipe the clones on exit
rm -f "/tmp/kaesar.sh"

if [ ${RETURN_VALUE} -eq ${KO_RC_REBOOT} ]; then
    echo "Rebooting..." >>crash.log 2>&1
    reboot
fi

if [ ${RETURN_VALUE} -eq ${KO_RC_HALT} ]; then
    echo "Powering off..." >>crash.log 2>&1
    poweroff -f
fi

exit ${RETURN_VALUE}
