-- TOFU host-key pinning: remember the SSH server key fingerprint observed on
-- first successful connect; later connects must match or the panel refuses to
-- talk to the node (defends against a swapped/MITM'd host).
ALTER TABLE nodes ADD COLUMN ssh_host_fingerprint TEXT;
