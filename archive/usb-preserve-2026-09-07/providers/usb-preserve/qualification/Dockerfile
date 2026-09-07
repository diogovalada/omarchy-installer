# Development/qualification only. No host devices are passed into this image.
FROM omarchy-local-image-proof:4.0.2
USER root
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends grub-efi-amd64-bin=2.12-1ubuntu7.3 grub-common=2.12-1ubuntu7.3 ntfs-3g gdisk exfatprogs \
    && dpkg-query -W > /usb-qualification-packages.txt
ENTRYPOINT ["python3"]
