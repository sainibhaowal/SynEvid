#!/usr/bin/env python3
"""
Generates 50 deterministic benchmark tasks conforming to schemas/benchmark-task.schema.json.
Tasks cover:
- api_change (1-10)
- migration (11-20)
- relayer (21-30)
- cross_package (31-40)
- dead_call & dynamic (41-50)
"""

import json
from pathlib import Path

TASKS = [
    # -------------------------------------------------------------------------
    # 1. API Changes (Tasks 1 - 10) in repo-api
    # -------------------------------------------------------------------------
    {
        "id": "TASK-API-001",
        "repository": "repo-api",
        "category": "api_change",
        "base_ref": "HEAD",
        "prompt": "Add a new required parameter 'tenantId: string' to UserService.createUser.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/services/userService.ts::UserService::createUser"],
            "expected_impacted_symbols": [
                "src/controllers/userController.ts::UserController::handleCreate",
                "src/routes/userRoutes.ts::routeRequest"
            ],
            "expected_failure": True,
            "expected_failure_classes": ["breaking_contract_required_param"],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-API-002",
        "repository": "repo-api",
        "category": "api_change",
        "base_ref": "HEAD",
        "prompt": "Add an optional parameter 'tenantId?: string' to UserService.createUser.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/services/userService.ts::UserService::createUser"],
            "expected_impacted_symbols": [
                "src/controllers/userController.ts::UserController::handleCreate"
            ],
            "expected_failure": False,
            "expected_failure_classes": [],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-API-003",
        "repository": "repo-api",
        "category": "api_change",
        "base_ref": "HEAD",
        "prompt": "Remove method deleteUser from UserService.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/services/userService.ts::UserService::deleteUser"],
            "expected_impacted_symbols": [],
            "expected_failure": False,
            "expected_failure_classes": ["removed_public_method"],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-API-004",
        "repository": "repo-api",
        "category": "api_change",
        "base_ref": "HEAD",
        "prompt": "Rename exported function hashPassword in crypto.ts to hashSecret.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/utils/crypto.ts::hashPassword"],
            "expected_impacted_symbols": [
                "src/utils/crypto.ts::verifyPassword",
                "src/services/userService.ts::UserService::createUser",
                "src/controllers/userController.ts::UserController::handleCreate"
            ],
            "expected_failure": True,
            "expected_failure_classes": ["breaking_symbol_rename"],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-API-005",
        "repository": "repo-api",
        "category": "api_change",
        "base_ref": "HEAD",
        "prompt": "Change return type of authenticateUser from string | null to boolean.",
        "is_held_out": True,
        "ground_truth": {
            "seed_symbols": ["src/auth/authGuard.ts::authenticateUser"],
            "expected_impacted_symbols": [],
            "expected_failure": True,
            "expected_failure_classes": ["breaking_contract_return_type"],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-API-006",
        "repository": "repo-api",
        "category": "api_change",
        "base_ref": "HEAD",
        "prompt": "Add required property 'phoneNumber: string' to CreateUserDto interface.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/dto/userDto.ts::CreateUserDto"],
            "expected_impacted_symbols": [
                "src/services/userService.ts::UserService::createUser",
                "src/controllers/userController.ts::UserController::handleCreate",
                "src/routes/userRoutes.ts::routeRequest"
            ],
            "expected_failure": True,
            "expected_failure_classes": ["breaking_interface_required_prop"],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-API-007",
        "repository": "repo-api",
        "category": "api_change",
        "base_ref": "HEAD",
        "prompt": "Add optional property 'avatarUrl?: string' to UserResponseDto.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/dto/userDto.ts::UserResponseDto"],
            "expected_impacted_symbols": [
                "src/services/userService.ts::UserService::createUser",
                "src/controllers/userController.ts::UserController::handleCreate"
            ],
            "expected_failure": False,
            "expected_failure_classes": [],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-API-008",
        "repository": "repo-api",
        "category": "api_change",
        "base_ref": "HEAD",
        "prompt": "Change internal hashing salt inside crypto.ts::hashPassword without signature changes.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/utils/crypto.ts::hashPassword"],
            "expected_impacted_symbols": [
                "src/utils/crypto.ts::verifyPassword",
                "src/services/userService.ts::UserService::createUser"
            ],
            "expected_failure": False,
            "expected_failure_classes": [],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-API-009",
        "repository": "repo-api",
        "category": "api_change",
        "base_ref": "HEAD",
        "prompt": "Add internal private method 'validateEmailFormat' to UserService.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/services/userService.ts::UserService"],
            "expected_impacted_symbols": [],
            "expected_failure": False,
            "expected_failure_classes": [],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-API-010",
        "repository": "repo-api",
        "category": "api_change",
        "base_ref": "HEAD",
        "prompt": "Transitive query: Trace impact from crypto.ts::generateToken to callers.",
        "is_held_out": True,
        "ground_truth": {
            "seed_symbols": ["src/utils/crypto.ts::generateToken"],
            "expected_impacted_symbols": [
                "src/auth/authGuard.ts::authenticateUser"
            ],
            "expected_failure": False,
            "expected_failure_classes": [],
            "expected_coverage": "verified"
        }
    },

    # -------------------------------------------------------------------------
    # 2. Migrations (Tasks 11 - 20) in repo-migration
    # -------------------------------------------------------------------------
    {
        "id": "TASK-MIG-011",
        "repository": "repo-migration",
        "category": "migration",
        "base_ref": "HEAD",
        "prompt": "NewStorage in modern/ directly imports OldStorage from legacy/.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/modern/newStorage.ts::NewStorage"],
            "expected_impacted_symbols": ["src/legacy/oldStorage.ts::OldStorage"],
            "expected_failure": True,
            "expected_failure_classes": ["deprecated-storage-prohibition"],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-MIG-012",
        "repository": "repo-migration",
        "category": "migration",
        "base_ref": "HEAD",
        "prompt": "SqlAdapter in adapters/ imports OldStorage instead of NewStorage.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/adapters/sqlAdapter.ts::SqlAdapter"],
            "expected_impacted_symbols": ["src/legacy/oldStorage.ts::OldStorage"],
            "expected_failure": False,
            "expected_failure_classes": [],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-MIG-013",
        "repository": "repo-migration",
        "category": "migration",
        "base_ref": "HEAD",
        "prompt": "Unambiguously rename src/legacy/oldStorage.ts to src/legacy/deprecatedStorage.ts.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/legacy/oldStorage.ts"],
            "expected_impacted_symbols": [
                "src/services/migrationRunner.ts::MigrationRunner"
            ],
            "expected_failure": False,
            "expected_failure_classes": [],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-MIG-014",
        "repository": "repo-migration",
        "category": "migration",
        "base_ref": "HEAD",
        "prompt": "Create ambiguous copy of oldStorage.ts under duplicate path backupStorage.ts.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/legacy/oldStorage.ts"],
            "expected_impacted_symbols": [],
            "expected_failure": False,
            "expected_failure_classes": ["ambiguous_rename_guard"],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-MIG-015",
        "repository": "repo-migration",
        "category": "migration",
        "base_ref": "HEAD",
        "prompt": "Change batch size in MigrationRunner.migrateAll.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/services/migrationRunner.ts::MigrationRunner::migrateAll"],
            "expected_impacted_symbols": [],
            "expected_failure": False,
            "expected_failure_classes": [],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-MIG-016",
        "repository": "repo-migration",
        "category": "migration",
        "base_ref": "HEAD",
        "prompt": "Modify RecordEntity payload from string to object type.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/domain/entity.ts::RecordEntity"],
            "expected_impacted_symbols": [
                "src/domain/entity.ts::createRecord",
                "src/legacy/oldStorage.ts::OldStorage::saveOld",
                "src/modern/newStorage.ts::NewStorage::insertRecord",
                "src/adapters/sqlAdapter.ts::SqlAdapter::executeInsert",
                "src/services/migrationRunner.ts::MigrationRunner::migrateAll"
            ],
            "expected_failure": True,
            "expected_failure_classes": ["breaking_contract_type"],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-MIG-017",
        "repository": "repo-migration",
        "category": "migration",
        "base_ref": "HEAD",
        "prompt": "Change OldStorage.saveOld signature to return Promise<void>.",
        "is_held_out": True,
        "ground_truth": {
            "seed_symbols": ["src/legacy/oldStorage.ts::OldStorage::saveOld"],
            "expected_impacted_symbols": [],
            "expected_failure": True,
            "expected_failure_classes": ["breaking_contract_async"],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-MIG-018",
        "repository": "repo-migration",
        "category": "migration",
        "base_ref": "HEAD",
        "prompt": "Update defaultDbConfig port to 5433.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/config/dbConfig.ts::defaultDbConfig"],
            "expected_impacted_symbols": [
                "src/adapters/sqlAdapter.ts::SqlAdapter"
            ],
            "expected_failure": False,
            "expected_failure_classes": [],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-MIG-019",
        "repository": "repo-migration",
        "category": "migration",
        "base_ref": "HEAD",
        "prompt": "Move entity.ts to src/domain/recordEntity.ts.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/domain/entity.ts"],
            "expected_impacted_symbols": [
                "src/legacy/oldStorage.ts",
                "src/modern/newStorage.ts",
                "src/adapters/sqlAdapter.ts"
            ],
            "expected_failure": False,
            "expected_failure_classes": [],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-MIG-020",
        "repository": "repo-migration",
        "category": "migration",
        "base_ref": "HEAD",
        "prompt": "Transitive query: Trace impact from NewStorage.insertRecord back to callers.",
        "is_held_out": True,
        "ground_truth": {
            "seed_symbols": ["src/modern/newStorage.ts::NewStorage::insertRecord"],
            "expected_impacted_symbols": [
                "src/adapters/sqlAdapter.ts::SqlAdapter::executeInsert",
                "src/services/migrationRunner.ts::MigrationRunner::migrateAll"
            ],
            "expected_failure": False,
            "expected_failure_classes": [],
            "expected_coverage": "verified"
        }
    },

    # -------------------------------------------------------------------------
    # 3. Relayering (Tasks 21 - 30) in repo-relayer
    # -------------------------------------------------------------------------
    {
        "id": "TASK-REL-021",
        "repository": "repo-relayer",
        "category": "relayer",
        "base_ref": "HEAD",
        "prompt": "DatabaseDriver in infrastructure/ imports BankingView from presentation/.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/infrastructure/database.ts::DatabaseDriver"],
            "expected_impacted_symbols": ["src/presentation/view.ts::BankingView"],
            "expected_failure": True,
            "expected_failure_classes": ["architecture-layers", "no_new_cycle"],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-REL-022",
        "repository": "repo-relayer",
        "category": "relayer",
        "base_ref": "HEAD",
        "prompt": "BankingView in presentation/ directly imports DatabaseDriver in infrastructure/.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/presentation/view.ts::BankingView"],
            "expected_impacted_symbols": ["src/infrastructure/database.ts::DatabaseDriver"],
            "expected_failure": True,
            "expected_failure_classes": ["architecture-layers"],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-REL-023",
        "repository": "repo-relayer",
        "category": "relayer",
        "base_ref": "HEAD",
        "prompt": "BankingDomain imports BankingView creating a direct cyclic dependency.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/domain/businessLogic.ts::BankingDomain"],
            "expected_impacted_symbols": ["src/presentation/view.ts::BankingView"],
            "expected_failure": True,
            "expected_failure_classes": ["no-circular-deps", "architecture-layers"],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-REL-024",
        "repository": "repo-relayer",
        "category": "relayer",
        "base_ref": "HEAD",
        "prompt": "BankingView calls BankingDomain.processTransfer (valid downward layering).",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/presentation/view.ts::BankingView::renderTransferForm"],
            "expected_impacted_symbols": ["src/domain/businessLogic.ts::BankingDomain::processTransfer"],
            "expected_failure": False,
            "expected_failure_classes": [],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-REL-025",
        "repository": "repo-relayer",
        "category": "relayer",
        "base_ref": "HEAD",
        "prompt": "Add required property 'fee: number' to TransactionRequest interface.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/core/contracts.ts::TransactionRequest"],
            "expected_impacted_symbols": [
                "src/domain/businessLogic.ts::BankingDomain::processTransfer",
                "src/presentation/view.ts::BankingView::renderTransferForm"
            ],
            "expected_failure": True,
            "expected_failure_classes": ["breaking_interface_required_prop"],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-REL-026",
        "repository": "repo-relayer",
        "category": "relayer",
        "base_ref": "HEAD",
        "prompt": "Change DatabaseDriver.updateBalance signature to require 'timestamp: number'.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/infrastructure/database.ts::DatabaseDriver::updateBalance"],
            "expected_impacted_symbols": [
                "src/domain/businessLogic.ts::BankingDomain::processTransfer",
                "src/presentation/view.ts::BankingView::renderTransferForm"
            ],
            "expected_failure": True,
            "expected_failure_classes": ["breaking_contract_required_param"],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-REL-027",
        "repository": "repo-relayer",
        "category": "relayer",
        "base_ref": "HEAD",
        "prompt": "Add permitted log call to shared logger from database driver.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/infrastructure/database.ts::DatabaseDriver"],
            "expected_impacted_symbols": ["src/shared/logger.ts::logInfo"],
            "expected_failure": False,
            "expected_failure_classes": [],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-REL-028",
        "repository": "repo-relayer",
        "category": "relayer",
        "base_ref": "HEAD",
        "prompt": "Trace backward blast radius for sanitizer.ts::sanitizeInput.",
        "is_held_out": True,
        "ground_truth": {
            "seed_symbols": ["src/security/sanitizer.ts::sanitizeInput"],
            "expected_impacted_symbols": [
                "src/presentation/view.ts::BankingView::renderTransferForm"
            ],
            "expected_failure": False,
            "expected_failure_classes": [],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-REL-029",
        "repository": "repo-relayer",
        "category": "relayer",
        "base_ref": "HEAD",
        "prompt": "Trace 2-hop transitive impact path from BankingView to DatabaseDriver.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/presentation/view.ts::BankingView"],
            "expected_impacted_symbols": [
                "src/domain/businessLogic.ts::BankingDomain",
                "src/infrastructure/database.ts::DatabaseDriver"
            ],
            "expected_failure": False,
            "expected_failure_classes": [],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-REL-030",
        "repository": "repo-relayer",
        "category": "relayer",
        "base_ref": "HEAD",
        "prompt": "Add indirect cycle between database.ts and logger.ts.",
        "is_held_out": True,
        "ground_truth": {
            "seed_symbols": ["src/shared/logger.ts::logInfo"],
            "expected_impacted_symbols": ["src/infrastructure/database.ts::DatabaseDriver"],
            "expected_failure": True,
            "expected_failure_classes": ["no-circular-deps"],
            "expected_coverage": "verified"
        }
    },

    # -------------------------------------------------------------------------
    # 4. Cross-Package (Tasks 31 - 40) in repo-cross-package
    # -------------------------------------------------------------------------
    {
        "id": "TASK-PKG-031",
        "repository": "repo-cross-package",
        "category": "cross_package",
        "base_ref": "HEAD",
        "prompt": "packages/core/ imports AppClient from packages/client/ (inverting architectural hierarchy).",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["packages/core/src/index.ts::EventBus"],
            "expected_impacted_symbols": ["packages/client/src/client.ts::AppClient"],
            "expected_failure": True,
            "expected_failure_classes": ["core-independence"],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-PKG-032",
        "repository": "repo-cross-package",
        "category": "cross_package",
        "base_ref": "HEAD",
        "prompt": "packages/core/ imports LoggingServerHandler from packages/server/.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["packages/core/src/index.ts::EventBus"],
            "expected_impacted_symbols": ["packages/server/src/server.ts::LoggingServerHandler"],
            "expected_failure": True,
            "expected_failure_classes": ["core-independence"],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-PKG-033",
        "repository": "repo-cross-package",
        "category": "cross_package",
        "base_ref": "HEAD",
        "prompt": "Modify MessageEnvelope interface in core adding required property 'priority: number'.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["packages/core/src/types.ts::MessageEnvelope"],
            "expected_impacted_symbols": [
                "packages/core/src/types.ts::EventHandler::handle",
                "packages/core/src/index.ts::EventBus::publish",
                "packages/client/src/client.ts::AppClient::sendNotification",
                "packages/server/src/server.ts::LoggingServerHandler::handle",
                "packages/plugin/src/plugin.ts::CustomPluginHandler::handle"
            ],
            "expected_failure": True,
            "expected_failure_classes": ["breaking_interface_required_prop"],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-PKG-034",
        "repository": "repo-cross-package",
        "category": "cross_package",
        "base_ref": "HEAD",
        "prompt": "Add optional property 'priority?: number' to MessageEnvelope in core.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["packages/core/src/types.ts::MessageEnvelope"],
            "expected_impacted_symbols": [
                "packages/core/src/index.ts::EventBus::publish",
                "packages/client/src/client.ts::AppClient::sendNotification"
            ],
            "expected_failure": False,
            "expected_failure_classes": [],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-PKG-035",
        "repository": "repo-cross-package",
        "category": "cross_package",
        "base_ref": "HEAD",
        "prompt": "Change EventHandler.handle signature to require extra parameter 'context: any'.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["packages/core/src/types.ts::EventHandler::handle"],
            "expected_impacted_symbols": [
                "packages/server/src/server.ts::LoggingServerHandler::handle",
                "packages/plugin/src/plugin.ts::CustomPluginHandler::handle",
                "packages/core/src/index.ts::EventBus::publish"
            ],
            "expected_failure": True,
            "expected_failure_classes": ["breaking_contract_required_param"],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-PKG-036",
        "repository": "repo-cross-package",
        "category": "cross_package",
        "base_ref": "HEAD",
        "prompt": "Add unsubscribe method to EventBus class in core package.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["packages/core/src/index.ts::EventBus"],
            "expected_impacted_symbols": [],
            "expected_failure": False,
            "expected_failure_classes": [],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-PKG-037",
        "repository": "repo-cross-package",
        "category": "cross_package",
        "base_ref": "HEAD",
        "prompt": "Update AppClient.sendNotification implementation to validate topic format.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["packages/client/src/client.ts::AppClient::sendNotification"],
            "expected_impacted_symbols": [],
            "expected_failure": False,
            "expected_failure_classes": [],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-PKG-038",
        "repository": "repo-cross-package",
        "category": "cross_package",
        "base_ref": "HEAD",
        "prompt": "Trace cross-package reachability from helpers.ts::serializePayload to AppClient.",
        "is_held_out": True,
        "ground_truth": {
            "seed_symbols": ["packages/utils/src/helpers.ts::serializePayload"],
            "expected_impacted_symbols": [
                "packages/client/src/client.ts::AppClient::sendNotification"
            ],
            "expected_failure": False,
            "expected_failure_classes": [],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-PKG-039",
        "repository": "repo-cross-package",
        "category": "cross_package",
        "base_ref": "HEAD",
        "prompt": "Add independent helper 'formatPayload' inside packages/utils/.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["packages/utils/src/helpers.ts"],
            "expected_impacted_symbols": [],
            "expected_failure": False,
            "expected_failure_classes": [],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-PKG-040",
        "repository": "repo-cross-package",
        "category": "cross_package",
        "base_ref": "HEAD",
        "prompt": "Create cross-package cycle: client.ts imports server.ts and server.ts imports client.ts.",
        "is_held_out": True,
        "ground_truth": {
            "seed_symbols": ["packages/client/src/client.ts::AppClient"],
            "expected_impacted_symbols": ["packages/server/src/server.ts::LoggingServerHandler"],
            "expected_failure": True,
            "expected_failure_classes": ["no-circular-deps"],
            "expected_coverage": "verified"
        }
    },

    # -------------------------------------------------------------------------
    # 5. Dead Call & Dynamic Constructs (Tasks 41 - 50) in repo-dead-call
    # -------------------------------------------------------------------------
    {
        "id": "TASK-DED-041",
        "repository": "repo-dead-call",
        "category": "dead_call",
        "base_ref": "HEAD",
        "prompt": "Modify uncalled obsoleteFormat in stringFormat.ts (evaluating dead-code blast radius precision).",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/helpers/stringFormat.ts::obsoleteFormat"],
            "expected_impacted_symbols": [
                "src/obsolete/deprecatedHandler.ts::DeprecatedHandler::executeLegacy"
            ],
            "expected_failure": False,
            "expected_failure_classes": [],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-DED-042",
        "repository": "repo-dead-call",
        "category": "dead_call",
        "base_ref": "HEAD",
        "prompt": "Delete unreferenced dead method DeprecatedHandler.executeLegacy.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/obsolete/deprecatedHandler.ts::DeprecatedHandler::executeLegacy"],
            "expected_impacted_symbols": [],
            "expected_failure": False,
            "expected_failure_classes": [],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-DED-043",
        "repository": "repo-dead-call",
        "category": "dead_call",
        "base_ref": "HEAD",
        "prompt": "Modify active helper capitalize in stringFormat.ts.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/helpers/stringFormat.ts::capitalize"],
            "expected_impacted_symbols": [
                "src/active/livePipeline.ts::LiveDataPipeline::processMessage",
                "src/analytics/telemetry.ts::TelemetryCollector::trackEvent",
                "src/cleanup/orphanCleaner.ts::OrphanCleaner::performCleanup"
            ],
            "expected_failure": False,
            "expected_failure_classes": [],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-DED-044",
        "repository": "repo-dead-call",
        "category": "dead_call",
        "base_ref": "HEAD",
        "prompt": "Verify file evalProxy.ts containing dynamic eval construct with --strict mode.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/dynamic/evalProxy.ts::dynamicCompute"],
            "expected_impacted_symbols": [],
            "expected_failure": True,
            "expected_failure_classes": ["unsupported_construct_eval"],
            "expected_coverage": "unknown"
        }
    },
    {
        "id": "TASK-DED-045",
        "repository": "repo-dead-call",
        "category": "dead_call",
        "base_ref": "HEAD",
        "prompt": "Active pipeline imports DeprecatedHandler violating clean-active-pipeline invariant.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/active/livePipeline.ts::LiveDataPipeline"],
            "expected_impacted_symbols": ["src/obsolete/deprecatedHandler.ts::DeprecatedHandler"],
            "expected_failure": True,
            "expected_failure_classes": ["clean-active-pipeline"],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-DED-046",
        "repository": "repo-dead-call",
        "category": "dead_call",
        "base_ref": "HEAD",
        "prompt": "Trace 3-hop transitive reachability from trimEllipsis down to OrphanCleaner.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/helpers/stringFormat.ts::trimEllipsis"],
            "expected_impacted_symbols": [
                "src/active/livePipeline.ts::LiveDataPipeline::processMessage",
                "src/analytics/telemetry.ts::TelemetryCollector::trackEvent",
                "src/cleanup/orphanCleaner.ts::OrphanCleaner::performCleanup"
            ],
            "expected_failure": False,
            "expected_failure_classes": [],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-DED-047",
        "repository": "repo-dead-call",
        "category": "dead_call",
        "base_ref": "HEAD",
        "prompt": "Detect generated code marker in generated stub file.",
        "is_held_out": True,
        "ground_truth": {
            "seed_symbols": ["src/dynamic/evalProxy.ts"],
            "expected_impacted_symbols": [],
            "expected_failure": False,
            "expected_failure_classes": [],
            "expected_coverage": "unknown"
        }
    },
    {
        "id": "TASK-DED-048",
        "repository": "repo-dead-call",
        "category": "dead_call",
        "base_ref": "HEAD",
        "prompt": "Modify leaf node OrphanCleaner.performCleanup.",
        "is_held_out": False,
        "ground_truth": {
            "seed_symbols": ["src/cleanup/orphanCleaner.ts::OrphanCleaner::performCleanup"],
            "expected_impacted_symbols": [],
            "expected_failure": False,
            "expected_failure_classes": [],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-DED-049",
        "repository": "repo-dead-call",
        "category": "dead_call",
        "base_ref": "HEAD",
        "prompt": "Query backward blast radius for TelemetryCollector.trackEvent.",
        "is_held_out": True,
        "ground_truth": {
            "seed_symbols": ["src/analytics/telemetry.ts::TelemetryCollector::trackEvent"],
            "expected_impacted_symbols": [
                "src/cleanup/orphanCleaner.ts::OrphanCleaner::performCleanup"
            ],
            "expected_failure": False,
            "expected_failure_classes": [],
            "expected_coverage": "verified"
        }
    },
    {
        "id": "TASK-DED-050",
        "repository": "repo-dead-call",
        "category": "dead_call",
        "base_ref": "HEAD",
        "prompt": "Simultaneous invariant verification and honest coverage evaluation in dead-call repo.",
        "is_held_out": True,
        "ground_truth": {
            "seed_symbols": ["src/helpers/stringFormat.ts::capitalize"],
            "expected_impacted_symbols": [
                "src/active/livePipeline.ts::LiveDataPipeline::processMessage",
                "src/analytics/telemetry.ts::TelemetryCollector::trackEvent"
            ],
            "expected_failure": False,
            "expected_failure_classes": [],
            "expected_coverage": "verified"
        }
    }
]

def main():
    tasks_dir = Path(__file__).parent / "tasks"
    tasks_dir.mkdir(parents=True, exist_ok=True)

    print(f"Generating {len(TASKS)} benchmark task specifications in {tasks_dir}...")
    for idx, task in enumerate(TASKS, start=1):
        filename = f"task_{idx:03d}.json"
        target_path = tasks_dir / filename
        with open(target_path, "w", encoding="utf-8") as f:
            json.dump(task, f, indent=2, sort_keys=True)
            f.write("\n")
    print(f"Successfully generated all {len(TASKS)} benchmark tasks.")

if __name__ == "__main__":
    main()
