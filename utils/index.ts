import * as fs from 'node:fs'
import * as path from 'node:path'

const readme = path.resolve('../README.md')

type Directories = string[]

function getDirectories<T extends string | fs.PathLike>(directory: T): Directories {
  const directories: string[] = []
  const files = fs.readdirSync(directory)

  files.forEach((file) => {
    const _path = path.join(directory as string, file)
    if (fs.statSync(_path).isDirectory() && !['node_modules', '.git', 'target', 'utils', '.github'].includes(file)) {
      const readmePath = path.join(_path, 'README.md')
      if (fs.existsSync(readmePath)) {
        directories.push(path.relative('..', _path))
      }
      directories.push(...getDirectories(_path))
    }
  })

  return directories
}

function addReadmes(): void {
  let contentMain = fs.readFileSync(readme, 'utf8')
  const readmeDirs = getDirectories('..')

  const start = '<!-- README-LIST:START -->'
  const end = '<!-- README-LIST:END -->'
  const links = readmeDirs
    .map((dirName) => `* [${dirName}](${dirName}/README.md)`)
    .join('\n')

  const regex = new RegExp(`${start}[\\s\\S]*?${end}`)
  if (!regex.test(contentMain)) {
    console.error('README markers not found')
    process.exit(1)
  }

  contentMain = contentMain.replace(regex, `${start}\n${links}\n${end}`)
  fs.writeFileSync(readme, contentMain, 'utf8')
}

addReadmes()
