const test = require('ava')
const fs = require('fs')
const path = require('path')

const bindings = require('../../index.node')
const napiVersion = require('../napi-version')

const filepath = path.resolve(__dirname, './example.txt')

test('should read a file and return its a buffer', async (t) => {
  if (napiVersion < 4) {
    t.is(bindings.testTokioReadfile, undefined)
    return
  }
  return new Promise((resolve, reject) => {
    bindings.testTokioReadfile(filepath, (err, value) => {
      try {
        t.is(err, null)
        t.is(Buffer.isBuffer(value), true)
        t.is(value.toString(), fs.readFileSync(filepath, 'utf8'))
        resolve()
      } catch (err) {
        reject(err)
      }
    })
  })
})
