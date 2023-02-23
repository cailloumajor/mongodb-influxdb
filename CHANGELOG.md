# Changelog

## [2.1.0](https://github.com/cailloumajor/mongodb-scraper/compare/v2.0.1...v2.1.0) (2023-02-23)


### Features

* stop using actix ([12e9a22](https://github.com/cailloumajor/mongodb-scraper/commit/12e9a224701692c2bbca5419d12da4be9387bc98))


### Bug Fixes

* add tests for health module ([8540819](https://github.com/cailloumajor/mongodb-scraper/commit/8540819409c11d8c928f0479a7f6a95c312cebbc))
* clone instead of re-computing local data ([7f847ca](https://github.com/cailloumajor/mongodb-scraper/commit/7f847cad23fd912bae63c48f9bde57bd0b85fa7f))
* **deps:** update rust crate clap to 4.1.6 ([f4a6391](https://github.com/cailloumajor/mongodb-scraper/commit/f4a639176375975257013cef19e2e7eac11ba4e6))
* **deps:** update rust crate signal-hook to 0.3.15 ([cf6be44](https://github.com/cailloumajor/mongodb-scraper/commit/cf6be44ed9e3670d2b899375fc3fcee5876d6867))
* **deps:** update rust crate tokio-stream to 0.1.12 ([ab4c297](https://github.com/cailloumajor/mongodb-scraper/commit/ab4c2972c5d17d51a408f69a5cc1eadbf18ef1a9))
* **deps:** update rust docker tag to v1.67.1 ([8db3023](https://github.com/cailloumajor/mongodb-scraper/commit/8db302366959930cf827ea83607d0fce45c44b74))
* **deps:** update tonistiigi/xx docker tag to v1.2.1 ([fe03239](https://github.com/cailloumajor/mongodb-scraper/commit/fe03239fcc2494c236527933a67a223d10a65717))
* fetch crates before build targets separation ([3d813db](https://github.com/cailloumajor/mongodb-scraper/commit/3d813dba4a27cd5d2b997104d3923acb69d6965e))

## [2.0.1](https://github.com/cailloumajor/mongodb-scraper/compare/v2.0.0...v2.0.1) (2023-02-08)


### Bug Fixes

* **deps:** update rust crate anyhow to 1.0.69 ([62b0e4f](https://github.com/cailloumajor/mongodb-scraper/commit/62b0e4fb8b6b75903e22a621079647eb666757bb))
* **deps:** update rust crate clap to 4.1.4 ([d189ef5](https://github.com/cailloumajor/mongodb-scraper/commit/d189ef551a77cb817e32c067d08d4b63dd23887e))
* **deps:** update rust crate futures-util to 0.3.26 ([5f50a48](https://github.com/cailloumajor/mongodb-scraper/commit/5f50a48fbec71680260a22670a201311fff303ef))
* **deps:** update rust crate tokio to 1.24.2 ([a401f46](https://github.com/cailloumajor/mongodb-scraper/commit/a401f466cfe183cd4815fe44e0027ce0bb536196))
* **deps:** update rust crate tokio to 1.25.0 ([d2793f9](https://github.com/cailloumajor/mongodb-scraper/commit/d2793f9f1df0f70c71bc4020771edbb9e67dc901))
* **deps:** update rust crate trillium-client to 0.3.0 ([cfb1793](https://github.com/cailloumajor/mongodb-scraper/commit/cfb1793e2d32389386da269a245a77c570c2323f))
* **deps:** update rust docker tag to v1.67.0 ([a670b8d](https://github.com/cailloumajor/mongodb-scraper/commit/a670b8d8ed0205c44e0218dd74487ea42a3d7884))
* **deps:** update tonistiigi/xx docker tag to v1.2.0 ([ff2d92d](https://github.com/cailloumajor/mongodb-scraper/commit/ff2d92d64d51efaaf5d5ca44fb6a6e2d22f8b01b))
* trillium-client 0.3.0 breaking changes ([9e32209](https://github.com/cailloumajor/mongodb-scraper/commit/9e32209f4d177ed684ef230d994e016c883239f9))
* use xx-cargo ([e21feca](https://github.com/cailloumajor/mongodb-scraper/commit/e21feca4bd4aca75289b4ab8d381f888c1d3e3e0))

## [2.0.0](https://github.com/cailloumajor/mongodb-scraper/compare/v1.1.0...v2.0.0) (2023-01-15)


### ⚠ BREAKING CHANGES

* follow MongoDB data document schema change

### Features

* follow MongoDB data document schema change ([49f0b3e](https://github.com/cailloumajor/mongodb-scraper/commit/49f0b3efe28caf43ae139110f7cbf833d735ae05))


### Bug Fixes

* **deps:** update rust crate clap to 4.1.1 ([4f16744](https://github.com/cailloumajor/mongodb-scraper/commit/4f1674465c7c64189f6430781369f636453e8b29))
* **deps:** update rust docker tag to v1.66.1 ([78c502d](https://github.com/cailloumajor/mongodb-scraper/commit/78c502dd53a9c1e4415604f819e2526d9f030ef8))
* follow changes in clap ([f54703e](https://github.com/cailloumajor/mongodb-scraper/commit/f54703e01d6c6eae7208691efa18c09744b64f0d))

## [1.1.0](https://github.com/cailloumajor/mongodb-scraper/compare/v1.0.1...v1.1.0) (2023-01-11)


### Features

* implement age fields ([382281f](https://github.com/cailloumajor/mongodb-scraper/commit/382281f0af78217f6e608dc237b0adbc8ab7441b))


### Bug Fixes

* **deps:** update rust crate anyhow to 1.0.68 ([43c946c](https://github.com/cailloumajor/mongodb-scraper/commit/43c946c502006b4af3b5e29063d72cf12806c9ce))
* **deps:** update rust crate clap to 4.0.32 ([e0799ab](https://github.com/cailloumajor/mongodb-scraper/commit/e0799ab128ee0c3567b29e312b4570026943a657))
* **deps:** update rust crate itoa to 1.0.5 ([ac01914](https://github.com/cailloumajor/mongodb-scraper/commit/ac01914b45972db55d55462813183a8620978a58))
* **deps:** update rust crate ryu to 1.0.12 ([9db4d23](https://github.com/cailloumajor/mongodb-scraper/commit/9db4d23a9f6f036b92b35b6ca050fabf6043f3c1))
* **deps:** update rust crate serde to 1.0.152 ([e636c5b](https://github.com/cailloumajor/mongodb-scraper/commit/e636c5b5a891384bbd3e47c0cad7229494519873))
* **deps:** update rust crate tokio to 1.24.1 ([87581c8](https://github.com/cailloumajor/mongodb-scraper/commit/87581c8a1829979b81e58f1a09a5fdd9a881c98f))

## [1.0.1](https://github.com/cailloumajor/mongodb-scraper/compare/v1.0.0...v1.0.1) (2022-12-16)


### Bug Fixes

* **deps:** update rust crate serde to 1.0.150 ([55b7c46](https://github.com/cailloumajor/mongodb-scraper/commit/55b7c463c33cf2b74c40de79ac8621514ee9615e))
* **deps:** update rust docker tag to v1.66.0 ([6532e9f](https://github.com/cailloumajor/mongodb-scraper/commit/6532e9f7d51f52dbfffe7c17b8de3dce41e98460))
* use tuple destructuring as self-documenting ([c10762c](https://github.com/cailloumajor/mongodb-scraper/commit/c10762c48a11a6c9cb764cf6aa82a4d36382f244))

## 1.0.0 (2022-12-07)


### Features

* implement healthcheck ([a2d55b2](https://github.com/cailloumajor/mongodb-scraper/commit/a2d55b27fe6383b6d6f522e8a7d9f5c50006119d))


### Bug Fixes

* calculate freshness against tick interval ([37d634c](https://github.com/cailloumajor/mongodb-scraper/commit/37d634cad8988b20788a469f5c635022e3f52513))
* **deps:** update rust crate clap to 4.0.29 ([879c199](https://github.com/cailloumajor/mongodb-scraper/commit/879c1999ad89fa8b7541119111267a79ccf47780))
* **deps:** update rust crate serde to 1.0.149 ([8abd2f3](https://github.com/cailloumajor/mongodb-scraper/commit/8abd2f3c71778987515d0df30e63ea8ce1aabe30))
* handle termination signals ([5334c15](https://github.com/cailloumajor/mongodb-scraper/commit/5334c15ccfd4a6e689853d6754d6208716bcc8ef))
* run the system ([a616916](https://github.com/cailloumajor/mongodb-scraper/commit/a6169163f85a067a304a96a6f8decc5ea874765e))
* stick with bson type for deserialization ([531273b](https://github.com/cailloumajor/mongodb-scraper/commit/531273b25f5606b55eeb9bec7ee963ce277adb7f))


### Miscellaneous Chores

* release 1.0.0 ([cf44261](https://github.com/cailloumajor/mongodb-scraper/commit/cf442618e9a13fd368429df910b3ec3a7d2c6a3e))
