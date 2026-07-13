import os
import subprocess
import sys
import tempfile

VMLINUX_C = '''\
struct sk_buff {
    unsigned int len;
};

struct sock {};

int napi_gro_receive(struct sk_buff *skb)
{
    return skb->len;
}

int tcp_v4_do_rcv(struct sock *sk, struct sk_buff *skb)
{
    return skb->len;
}
'''


def main():
    root = os.path.dirname(os.path.dirname(__file__))
    out_dir = os.path.join(root, 'tests', 'data')
    os.makedirs(out_dir, exist_ok=True)

    with tempfile.TemporaryDirectory() as tmp:
        c_path = os.path.join(tmp, 'vmlinux.c')
        o_path = os.path.join(tmp, 'vmlinux.o')
        btf_path = os.path.join(out_dir, 'vmlinux')

        with open(c_path, 'w') as f:
            f.write(VMLINUX_C)

        for cmd in [
            ['clang', '-g', '-c', c_path, '-o', o_path],
            ['pahole', '--btf_encode', o_path],
            ['llvm-objcopy', '--dump-section', f'.BTF={btf_path}', o_path],
        ]:
            print(f'-> {" ".join(cmd)}')
            r = subprocess.run(cmd)
            if r.returncode != 0:
                print(f'error: command failed with exit code {r.returncode}',
                      file=sys.stderr)
                sys.exit(r.returncode)

        print(f'generated "{btf_path}"')


if __name__ == '__main__':
    main()
