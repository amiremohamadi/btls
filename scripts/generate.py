import os
import subprocess

import mistune
from mistune.plugins import plugin_table


def _get_bpftrace_stdlib_docs():
    url = 'https://raw.githubusercontent.com/bpftrace/bpftrace/efb8d0d8876295f77170ec9a3c65101d8749f8db/docs/stdlib.md'
    r = subprocess.run(['curl', '-sSl', url], capture_output=True, text=True)
    return r.stdout


def _parse_vars_row(row):
    var_name = row['children'][0]
    var_type = row['children'][1]
    description = row['children'][3]

    var_name = var_name['children'][0].get('text', None)
    var_type = var_type['children'][0]['text']
    description = description['children'][0]['text']

    # ignore texts like '$1, $2, ...$n' and 'arg0, arg1, ...argn' for now
    if not var_name or ',' in var_name:
        return None

    return {
        'name': var_name,
        'type': var_type,
        'description': description,
    }


def _parse_vars_table(ast):
    table = next((n for n in ast if n['type'] == 'table'))
    rows = [row for entry in table['children'] for row in entry['children']
                if row['type'] == 'table_row']
    res = []
    for row in rows:
        if var := _parse_vars_row(row):
            res.append(var)
    return res


def generate_builtins():
    with open('./stdlib.md', 'r') as f:
        markdown = mistune.create_markdown(renderer=mistune.AstRenderer(),
                                           plugins=[plugin_table])
        ast = markdown(f.read())
        builtin_vars = _parse_vars_table(ast)

    with open('./target/builtins.gen.rs', 'w') as target:
        print('// DO NOT EDIT -- this file is auto generated\n',
              file=target)
        for var in builtin_vars:
            print(var)


def main():
    # os.chdir(os.path.dirname(os.path.dirname(__file__)))
    # print(_get_bpftrace_stdlib_docs())
    generate_builtins()


if __name__ == '__main__':
    main()
